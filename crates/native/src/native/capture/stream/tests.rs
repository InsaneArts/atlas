use super::*;

/// Resource-only probe: no Capture message, source, pixels, or input is created.
/// Run alone with --ignored --exact --nocapture to avoid unrelated test CPU.
#[test]
#[ignore = "manual resource comparison of an empty native worker, without capture"]
fn idle_worker_resource_probe_never_requests_native_capture() {
    fn usage() -> (f64, i64, i64) {
        let mut result = std::mem::MaybeUninit::<libc::rusage>::uninit();
        assert_eq!(
            // SAFETY: getrusage initializes this correctly sized output on success.
            unsafe { libc::getrusage(libc::RUSAGE_SELF, result.as_mut_ptr()) },
            0
        );
        // SAFETY: successful getrusage initialized every field above.
        let result = unsafe { result.assume_init() };
        let seconds = |time: libc::timeval| time.tv_sec as f64 + time.tv_usec as f64 / 1_000_000.0;
        (
            seconds(result.ru_utime) + seconds(result.ru_stime),
            result.ru_nvcsw,
            result.ru_nivcsw,
        )
    }
    fn sample(label: &str) {
        let start = Instant::now();
        let before = usage();
        std::thread::sleep(Duration::from_secs(2));
        let after = usage();
        eprintln!(
            "{label}: wall_ms={:.2} cpu_ms={:.3} voluntary_switches={} involuntary_switches={}",
            start.elapsed().as_secs_f64() * 1000.0,
            (after.0 - before.0) * 1000.0,
            after.1 - before.1,
            after.2 - before.2,
        );
    }
    sample("baseline");
    let slot = Arc::new(Slot::default());
    let worker_slot = slot.clone();
    let (send, receive) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || run(receive, worker_slot));
    // Exclude launchctl manager lookup and CF run-loop initialization.
    std::thread::sleep(Duration::from_secs(1));
    sample("empty-native-worker");
    assert_eq!(slot.diagnostics.snapshot()["requests"], 0);
    assert_eq!(slot.diagnostics.snapshot()["setupStarts"], 0);
    assert_eq!(slot.diagnostics.snapshot()["callbacks"], 0);
    assert!(slot.pending.lock().unwrap().is_none());
    drop(send);
    worker.join().unwrap();
}

#[test]
fn native_objects_preparation_drain_and_any_pending_request_prevent_quiescence() {
    let slot = Slot::default();
    assert!(slot.quiescent(false));
    assert!(!slot.quiescent(true));
    slot.preparing.store(true, Ordering::Release);
    assert!(!slot.quiescent(false));
    slot.preparing.store(false, Ordering::Release);
    slot.stopping.store(true, Ordering::Release);
    assert!(!slot.quiescent(false));
    slot.stopping.store(false, Ordering::Release);
    let (pending, _reply) = request(1);
    let alive = pending.alive.clone();
    slot.begin(pending);
    assert!(!slot.quiescent(false));
    alive.store(false, Ordering::Release);
    assert!(
        !slot.quiescent(false),
        "pending cancellation must finish first"
    );
    slot.cancel();
    assert!(slot.quiescent(false));
}

#[test]
fn a_capture_wakes_the_quiescent_receiver_and_preparation_restores_timed_polling() {
    let slot = Arc::new(Slot::default());
    let (send, incoming) = mpsc::sync_channel(1);
    let (started, ready) = mpsc::sync_channel(1);
    let (finished, outcome) = mpsc::sync_channel(1);
    let worker_slot = slot.clone();
    let worker = std::thread::spawn(move || {
        started.send(()).unwrap();
        let result = receive(&incoming, &worker_slot, false);
        finished.send(()).unwrap();
        let Message::Capture(pending) = result.unwrap() else {
            panic!("expected Capture")
        };
        worker_slot.preparing.store(true, Ordering::Release);
        worker_slot.begin(pending);
        assert!(matches!(
            receive(&incoming, &worker_slot, false),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        worker_slot.preparing.store(false, Ordering::Release);
        // The source now owns a native stream even after its reply is sent.
        worker_slot.finish(1, Ok(shot(42)));
        assert!(matches!(
            receive(&incoming, &worker_slot, true),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(
            worker_slot.quiescent(false),
            "confirmed drain permits quiescence"
        );
    });
    ready.recv().unwrap();
    assert!(
        matches!(
            outcome.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "an empty owner must block beyond the 4 ms active poll"
    );
    let (pending, reply) = request(1);
    send.send(Message::Capture(pending)).unwrap();
    outcome.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(reply.recv().unwrap().unwrap().rgb, vec![42; 24]);
    worker.join().unwrap();
}

#[test]
fn explicit_stop_and_sender_departure_wake_a_quiescent_receiver() {
    for stop in [true, false] {
        let slot = Arc::new(Slot::default());
        let worker_slot = slot.clone();
        let (send, incoming) = mpsc::sync_channel(1);
        let (started, ready) = mpsc::sync_channel(1);
        let (finished, outcome) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            started.send(()).unwrap();
            let result = receive(&incoming, &worker_slot, false);
            finished
                .send(matches!(result, Ok(Message::Stop(_))))
                .unwrap();
        });
        ready.recv().unwrap();
        assert!(matches!(
            outcome.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        if stop {
            slot.stop();
            let (ack, _reply) = mpsc::sync_channel(1);
            send.send(Message::Stop(ack)).unwrap();
        } else {
            drop(send);
        }
        assert_eq!(outcome.recv_timeout(Duration::from_secs(1)).unwrap(), stop);
        worker.join().unwrap();
    }
}

fn key() -> Key {
    Key {
        epoch: InputEpoch {
            uid: 502,
            security_session_id: 123,
            display_id: 9,
            width: 1600.0,
            height: 1000.0,
            process_id: 77,
        },
        width: 4,
        height: 2,
        scale: 2.0,
    }
}

fn request(id: u64) -> (Pending, Receiver<Result<RawShot>>) {
    let (reply, receive) = mpsc::sync_channel(1);
    (
        Pending {
            id,
            checkpoint: 500,
            key: key(),
            alive: Arc::new(AtomicBool::new(true)),
            reply,
        },
        receive,
    )
}

fn shot(value: u8) -> RawShot {
    RawShot {
        rgb: Buffer::copy_from(&[value; 24]).unwrap(),
        width: 4,
        height: 2,
        scale: 2.0,
    }
}

#[test]
fn only_complete_samples_strictly_after_the_mach_checkpoint_are_eligible() {
    let complete = SCFrameStatus::Complete.0 as i64;
    assert!(accepts(complete, 501, 500, 1000));
    for (time, checkpoint, now) in [
        (500, 500, 1000),
        (499, 500, 1000),
        (1001, 500, 1000),
        (1, 0, 1000),
        (501, 500, 499),
    ] {
        assert!(!accepts(complete, time, checkpoint, now));
    }
    for status in [
        SCFrameStatus::Idle,
        SCFrameStatus::Blank,
        SCFrameStatus::Started,
        SCFrameStatus::Stopped,
        SCFrameStatus::Suspended,
    ] {
        assert!(!accepts(status.0 as i64, 501, 500, 1000));
    }
}

#[test]
fn every_part_of_the_exact_native_epoch_remains_required() {
    let expected = key().epoch;
    assert!(validate_epoch(&expected, &expected).is_ok());
    for field in 0..6 {
        let mut changed = expected.clone();
        match field {
            0 => changed.uid += 1,
            1 => changed.security_session_id += 1,
            2 => changed.display_id += 1,
            3 => changed.width += 1.0,
            4 => changed.height += 1.0,
            _ => changed.process_id += 1,
        }
        assert_eq!(
            validate_epoch(&changed, &expected).unwrap_err().code,
            Code::SessionChanged
        );
    }
}

#[test]
fn replacing_one_pending_request_cancels_it_and_cannot_deliver_its_old_result() {
    let slot = Slot::default();
    let (first, first_reply) = request(1);
    let alive = first.alive.clone();
    slot.begin(first);
    let (second, second_reply) = request(2);
    slot.begin(second);
    assert!(!alive.load(Ordering::Acquire));
    assert_eq!(
        first_reply
            .recv()
            .unwrap()
            .err()
            .expect("cancelled old request")
            .code,
        Code::Cancelled
    );
    slot.finish(1, Ok(shot(1)));
    assert!(matches!(
        second_reply.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    slot.finish(2, Ok(shot(2)));
    assert_eq!(second_reply.recv().unwrap().unwrap().rgb, vec![2; 24]);
    assert!(slot.pending.lock().unwrap().is_none());
}

#[test]
fn cancelling_during_a_pixel_copy_prevents_its_late_pixels_from_escaping() {
    let slot = Slot::default();
    let (pending, reply) = request(1);
    let alive = pending.alive.clone();
    slot.begin(pending);
    alive.store(false, Ordering::Release);
    slot.finish(1, Ok(shot(42)));
    assert!(matches!(
        reply.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    assert!(slot.pending.lock().unwrap().is_none());
}

#[test]
fn guard_failure_invalidates_the_generation_and_retains_its_permission_error() {
    let slot = Slot::default();
    let (pending, reply) = request(1);
    slot.begin(pending);
    let generation = slot.generation.load(Ordering::Acquire);
    slot.fail(Error::new(Code::Permission, "consent revoked"));
    assert!(slot.stopping.load(Ordering::Acquire));
    assert!(slot.generation.load(Ordering::Acquire) > generation);
    assert_eq!(
        reply
            .recv()
            .unwrap()
            .err()
            .expect("permission refusal")
            .code,
        Code::Permission
    );
    assert_eq!(slot.failure().unwrap().code, Code::Permission);
    assert!(!fallback_allowed(&slot.failure().unwrap()));
}

#[test]
fn a_live_owner_survives_request_gaps_but_explicit_close_cancels_delivery() {
    let slot = Slot::default();
    let start = Instant::now();
    let mut watchdog = Watchdog(start);
    let (pending, reply) = request(1);
    let alive = pending.alive.clone();
    slot.begin(pending);
    let mut checks = 0;
    // Simulate a connected reader applying backpressure for several seconds.
    // Time passing must only trigger security checks, never cancel ownership.
    for milliseconds in [19, 20, 500, 1001, 2000, 9000] {
        watchdog.poll(start + Duration::from_millis(milliseconds), &slot, || {
            checks += 1;
            Ok(())
        });
        assert!(!slot.stopping.load(Ordering::Acquire));
        assert_eq!(slot.generation.load(Ordering::Acquire), 0);
        assert!(alive.load(Ordering::Acquire));
        assert!(matches!(reply.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
    assert_eq!(checks, 5);
    slot.stop();
    assert!(slot.stopping.load(Ordering::Acquire));
    assert_eq!(slot.generation.load(Ordering::Acquire), 1);
    assert!(!alive.load(Ordering::Acquire));
    assert_eq!(reply.recv().unwrap().err().unwrap().code, Code::Cancelled);
    // A callback finishing after close cannot give pixels new authority.
    slot.finish(1, Ok(shot(42)));
    assert!(slot.pending.lock().unwrap().is_none());
}

#[test]
fn security_watchdog_stops_an_idle_owner_and_preserves_the_original_refusal() {
    for code in [
        Code::OnConsole,
        Code::Identity,
        Code::SessionChanged,
        Code::Permission,
    ] {
        let slot = Slot::default();
        let start = Instant::now();
        let mut watchdog = Watchdog(start);
        let (pending, reply) = request(1);
        slot.begin(pending);
        watchdog.poll(start + Duration::from_secs(2), &slot, || {
            Err(Error::new(code, "synthetic refusal"))
        });
        assert!(slot.stopping.load(Ordering::Acquire));
        assert_eq!(slot.generation.load(Ordering::Acquire), 1);
        assert_eq!(reply.recv().unwrap().err().unwrap().code, code);
        assert_eq!(slot.failure().unwrap().code, code);
        assert_eq!(slot.diagnostics.snapshot()["guardErrors"], 1);
        // Stopping sources remain quarantined, without another guard/copy.
        watchdog.poll(start + Duration::from_secs(3), &slot, || {
            panic!("must await native stop and drain")
        });
    }
}

#[test]
fn unexpected_native_stop_is_counted_without_exposing_native_error_text() {
    let slot = Arc::new(Slot::default());
    let (pending, reply) = request(1);
    slot.begin(pending);
    let callback = Callback {
        key: key(),
        slot: slot.clone(),
        generation: 0,
        active: AtomicBool::new(true),
    };
    callback.stopped(|| Ok(()));
    assert!(slot.stopping.load(Ordering::Acquire));
    assert_eq!(reply.recv().unwrap().err().unwrap().code, Code::Capture);
    let stats = slot.diagnostics.snapshot();
    assert_eq!(stats["delegateStopErrors"], 1);
    assert_eq!(stats["nativeReason"], "stop-error");
    callback.stopped(|| panic!("duplicate callback cannot query another proof"));
    assert_eq!(slot.diagnostics.snapshot()["delegateStopErrors"], 1);
}

#[test]
fn native_stop_after_explicit_close_is_not_reported_as_a_source_failure() {
    let slot = Arc::new(Slot::default());
    let callback = Callback {
        key: key(),
        slot: slot.clone(),
        generation: 0,
        active: AtomicBool::new(true),
    };
    slot.stop();
    callback.stopped(|| panic!("obsolete generation must not query native facts"));
    assert!(slot.failure().is_none());
    assert_eq!(slot.diagnostics.snapshot()["delegateStopErrors"], 0);
}

#[test]
fn preparing_or_stopping_a_source_prevents_reuse_until_native_drain_completes() {
    let slot = Slot::default();
    slot.preparing.store(true, Ordering::Release);
    assert!(
        !stop_native(&slot, false).unwrap(),
        "an outstanding native setup remains quarantined"
    );
    slot.generation.fetch_add(1, Ordering::AcqRel);
    slot.preparing.store(false, Ordering::Release);
    assert!(
        stop_native(&slot, false).unwrap(),
        "no native objects remain after setup cancellation"
    );
}

#[test]
fn only_capture_availability_errors_can_fall_back_to_the_guarded_source() {
    assert!(fallback_allowed(&capture_error("idle timeout")));
    for code in [
        Code::Identity,
        Code::OnConsole,
        Code::SessionChanged,
        Code::Permission,
        Code::Cancelled,
        Code::Stopped,
        Code::Busy,
    ] {
        assert!(!fallback_allowed(&Error::new(code, "refused")));
    }
}

#[test]
fn failed_or_missing_stop_acknowledgements_never_confirm_a_native_source_is_stopped() {
    let (send, receive) = mpsc::sync_channel(2);
    let mut failed = None;
    assert!(!confirmed_stop(&receive, &mut failed, false).unwrap());
    send.send(Err(capture_error("native stop failed"))).unwrap();
    assert!(confirmed_stop(&receive, &mut failed, false).is_err());
    assert!(failed.is_some());
    send.send(Ok(())).unwrap();
    assert!(
        confirmed_stop(&receive, &mut failed, false).is_err(),
        "a failed source remains quarantined"
    );
    let (send, receive) = mpsc::sync_channel(1);
    drop(send);
    assert!(confirmed_stop(&receive, &mut None, false).is_err());
}

#[test]
fn only_an_explicit_successful_stop_acknowledgement_allows_callback_drain() {
    let (send, receive) = mpsc::sync_channel(1);
    send.send(Ok(())).unwrap();
    assert!(confirmed_stop(&receive, &mut None, false).unwrap());
}

/// A CoreVideo allocation containing synthetic colors, never a screen image.
fn sample() -> CFRetained<CMSampleBuffer> {
    use objc2_core_media::{
        CMSampleTimingInfo, CMVideoFormatDescription, CMVideoFormatDescriptionCreateForImageBuffer,
        kCMTimeInvalid,
    };
    use objc2_core_video::CVPixelBufferCreate;
    use std::ptr::NonNull;
    let mut raw_buffer = std::ptr::null_mut();
    // SAFETY: valid output pointer, opaque BGRA dimensions and no attributes.
    assert_eq!(
        // SAFETY: valid output pointer, BGRA dimensions, no attributes.
        unsafe {
            CVPixelBufferCreate(
                None,
                4,
                2,
                kCVPixelFormatType_32BGRA,
                None,
                NonNull::from(&mut raw_buffer),
            )
        },
        0
    );
    // SAFETY: a successful Create returned one owned CoreVideo reference.
    let buffer = unsafe { CFRetained::from_raw(NonNull::new(raw_buffer).unwrap()) };
    // SAFETY: exclusive access to the new synthetic buffer, with matching flags.
    assert_eq!(
        // SAFETY: exclusive new buffer; the matching unlock is below.
        unsafe { CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()) },
        0
    );
    let stride = CVPixelBufferGetBytesPerRow(&buffer);
    let length = CVPixelBufferGetDataSize(&buffer);
    assert!(length >= stride * 2);
    // SAFETY: the buffer is exclusively owned and locked; its allocation size
    // is reported by CoreVideo. Every byte is initialized before unlocking.
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(CVPixelBufferGetBaseAddress(&buffer).cast::<u8>(), length)
    };
    bytes.fill(99);
    for y in 0..2 {
        for x in 0..4 {
            let offset = y * stride + x * 4;
            bytes[offset..offset + 4].copy_from_slice(if y == 0 {
                &[0, 0, 255, 255]
            } else {
                &[255, 0, 0, 255]
            });
        }
    }
    // SAFETY: matched with the successful write lock; no mutable borrow remains.
    assert_eq!(
        // SAFETY: matches the write lock and ends all exclusive byte access.
        unsafe { CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()) },
        0
    );
    let mut raw_description: *const CMVideoFormatDescription = std::ptr::null();
    // SAFETY: a live pixel buffer and a valid output for its owned description.
    assert_eq!(
        // SAFETY: live image and valid owned format-description output.
        unsafe {
            CMVideoFormatDescriptionCreateForImageBuffer(
                None,
                &buffer,
                NonNull::from(&mut raw_description),
            )
        },
        0
    );
    // SAFETY: successful Create returned an owned format-description reference.
    let description =
        unsafe { CFRetained::from_raw(NonNull::new(raw_description.cast_mut()).unwrap()) };
    // SAFETY: fixed numeric timing and CoreMedia's documented invalid decode
    // timestamp for uncompressed frames already in presentation order.
    let mut timing = unsafe {
        CMSampleTimingInfo {
            duration: CMTime::new(1, 60),
            presentationTimeStamp: CMTime::new(1, 60),
            decodeTimeStamp: kCMTimeInvalid,
        }
    };
    let mut raw_sample = std::ptr::null_mut();
    // SAFETY: live image/description and valid timing/output pointers; the
    // sample retains its buffer independently before these locals are dropped.
    assert_eq!(
        // SAFETY: live image/description and valid timing/sample outputs.
        unsafe {
            CMSampleBuffer::create_ready_with_image_buffer(
                None,
                &buffer,
                &description,
                NonNull::from(&mut timing),
                NonNull::from(&mut raw_sample),
            )
        },
        0
    );
    // SAFETY: successful Create returned the sample's one owned reference.
    unsafe { CFRetained::from_raw(NonNull::new(raw_sample).unwrap()) }
}

#[test]
fn locked_corevideo_pixels_keep_exact_synthetic_colors_and_orientation() {
    let sample = sample();
    let rgb = pixels(&sample, 4, 2).unwrap();
    let mut expected = [255, 0, 0].repeat(4);
    expected.extend([0, 0, 255].repeat(4));
    assert_eq!(rgb, expected);
    assert!(
        pixels(&sample, 3, 2).is_err(),
        "dimension changes cannot reinterpret pixel storage"
    );
    // Re-reading proves the read-only lock was released symmetrically.
    assert_eq!(pixels(&sample, 4, 2).unwrap(), expected);
}

#[test]
fn missing_or_incomplete_sample_metadata_never_becomes_a_new_frame() {
    let sample = sample();
    assert_eq!(metadata(&sample).unwrap(), None);
    // SAFETY: CoreMedia returns mutable CF dictionaries when requested; their
    // elements are checked/cast to the documented property-list types below.
    let attachments = unsafe { sample.sample_attachments_array(true) }.unwrap();
    // SAFETY: sample attachments are CF objects, with mutable dictionaries.
    let attachments = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(attachments) };
    let dictionary = attachments
        .get(0)
        .unwrap()
        .downcast::<CFDictionary>()
        .unwrap();
    // SAFETY: CoreMedia documents each sample attachment dictionary as mutable.
    let dictionary = unsafe {
        CFRetained::cast_unchecked::<objc2_core_foundation::CFMutableDictionary<CFString, CFType>>(
            dictionary,
        )
    };
    // SAFETY: SDK-exported constant is a live immutable NSString.
    let status = unsafe { SCStreamFrameInfoStatus }.to_string();
    dictionary.set(&CFString::from_str(&status), CFNumber::new_i64(0).as_ref());
    assert_eq!(
        eligibility(metadata(&sample).unwrap().unwrap(), 500, 1000),
        Eligibility::Incomplete,
        "a complete status without displayTime is still unusable"
    );
    // SAFETY: SDK-exported constant is a live immutable NSString.
    let time = unsafe { SCStreamFrameInfoDisplayTime }.to_string();
    dictionary.set(&CFString::from_str(&time), CFNumber::new_i64(501).as_ref());
    assert_eq!(
        metadata(&sample).unwrap(),
        Some(Metadata {
            status: 0,
            time: Some(501)
        })
    );
}

#[test]
fn idle_metadata_is_only_a_fallback_hint_and_never_a_fresh_image() {
    let idle = Metadata {
        status: SCFrameStatus::Idle.0 as i64,
        time: None,
    };
    assert_eq!(eligibility(idle, 500, 1000), Eligibility::Idle);
    let complete_without_time = Metadata {
        status: SCFrameStatus::Complete.0 as i64,
        time: None,
    };
    assert_eq!(
        eligibility(complete_without_time, 500, 1000),
        Eligibility::Incomplete
    );
    assert_eq!(
        eligibility(
            Metadata {
                status: 0,
                time: Some(499)
            },
            500,
            1000
        ),
        Eligibility::Old
    );
    assert_eq!(
        eligibility(
            Metadata {
                status: 0,
                time: Some(1001)
            },
            500,
            1000
        ),
        Eligibility::Future
    );
    assert_eq!(
        eligibility(
            Metadata {
                status: 0,
                time: Some(501)
            },
            500,
            1000
        ),
        Eligibility::Frame
    );
}

#[test]
fn an_idle_callback_wakes_the_caller_for_a_new_capture_without_disabling_native_streaming() {
    let sample = sample();
    // SAFETY: this synthetic sample's mutable attachment array owns CF values.
    let attachments = unsafe { sample.sample_attachments_array(true) }.unwrap();
    // SAFETY: CoreMedia documents CF dictionaries in this attachment array.
    let attachments = unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(attachments) };
    let dictionary = attachments
        .get(0)
        .unwrap()
        .downcast::<CFDictionary>()
        .unwrap();
    // SAFETY: CoreMedia's sample-attachment dictionaries are mutable CF objects.
    let dictionary = unsafe {
        CFRetained::cast_unchecked::<objc2_core_foundation::CFMutableDictionary<CFString, CFType>>(
            dictionary,
        )
    };
    // SAFETY: the exported metadata key is a live immutable string.
    let status = unsafe { SCStreamFrameInfoStatus }.to_string();
    dictionary.set(
        &CFString::from_str(&status),
        CFNumber::new_i64(SCFrameStatus::Idle.0 as i64).as_ref(),
    );
    let slot = Arc::new(Slot::default());
    let (pending, reply) = request(1);
    slot.begin(pending);
    let callback = Callback {
        key: key(),
        slot: slot.clone(),
        generation: 0,
        active: AtomicBool::new(true),
    };
    callback.sample(&sample);
    let hint = reply
        .recv()
        .unwrap()
        .err()
        .expect("idle cannot return pixels");
    assert_eq!(hint.details["nativeFallback"], "idle");
    assert!(
        fallback_allowed(&hint),
        "caller may take a new guarded synchronous capture"
    );
    assert!(callback.active.load(Ordering::Acquire));
    assert!(!slot.stopping.load(Ordering::Acquire));
    assert_eq!(slot.generation.load(Ordering::Acquire), 0);
    assert!(slot.failure().is_none());
    assert_eq!(slot.diagnostics.snapshot()["idleHints"], 1);
    assert_eq!(slot.diagnostics.snapshot()["nativeFrames"], 0);
}

#[test]
fn native_diagnostics_are_bounded_metadata_without_error_text_or_pixel_contents() {
    let diagnostics = Diagnostics::default();
    diagnostics.requests.store(4, Ordering::Relaxed);
    diagnostics.native_frames.store(1, Ordering::Relaxed);
    diagnostics.fallbacks.store(3, Ordering::Relaxed);
    *diagnostics.last_fallback.lock().unwrap() = Some("native-error");
    diagnostics.error(
        "start-error",
        &capture_error("private window or file information")
            .with("nativeReason", "no-exact-display".into()),
    );
    let snapshot = diagnostics.snapshot();
    assert_eq!(snapshot["nativeFrames"], 1);
    assert_eq!(snapshot["fallbacks"], 3);
    assert_eq!(snapshot["nativeReason"], "no-exact-display");
    assert_eq!(snapshot["lastFallback"], "native-error");
    let encoded = serde_json::to_string(&snapshot).unwrap();
    assert!(!encoded.contains("private window"));
    assert!(
        encoded.len() < 1024,
        "stats must leave room in the bounded protocol header"
    );
}
