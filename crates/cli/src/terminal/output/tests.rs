use super::*;

#[derive(Default)]
struct Sink {
    bytes: Vec<u8>,
    largest: usize,
    writes: usize,
    flushes: usize,
    partial: usize,
    fail_write: Option<usize>,
    fail_flush: bool,
    interrupt_write: Option<usize>,
    interrupt_flush: bool,
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        self.largest = self.largest.max(bytes.len());
        if self.interrupt_write == Some(self.writes) {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        if self.fail_write == Some(self.writes) {
            return Err(io::Error::from_raw_os_error(libc::EIO));
        }
        let count = if self.partial == 0 {
            bytes.len()
        } else {
            bytes.len().min(self.partial)
        };
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        if self.interrupt_flush {
            self.interrupt_flush = false;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        if self.fail_flush {
            self.fail_flush = false;
            Err(io::Error::other("first flush failed"))
        } else {
            Ok(())
        }
    }
}
fn occurrences(bytes: &[u8], needle: &[u8]) -> usize {
    bytes
        .windows(needle.len())
        .filter(|value| *value == needle)
        .count()
}

#[test]
fn complete_frames_have_exactly_one_sync_scope_and_count_actual_output() {
    let mut sink = Sink::default();
    let sent = frame(&mut sink, |out| {
        out.write_all(b"first")?;
        out.write_all(b"second")
    })
    .unwrap();
    assert_eq!(sink.bytes, [BEGIN, b"firstsecond", END].concat());
    assert_eq!(sent.bytes, sink.bytes.len());
    assert_eq!(occurrences(&sink.bytes, BEGIN), 1);
    assert_eq!(occurrences(&sink.bytes, END), 1);
    assert_eq!(sink.flushes, 1);
    assert!(sent.write_ms.is_finite() && sent.write_ms >= 0.0);
}

#[test]
fn empty_work_and_empty_writes_never_enter_sync_or_flush_the_sink() {
    let mut sink = Sink::default();
    let sent = frame(&mut sink, |out| {
        assert_eq!(out.write(b"")?, 0);
        out.flush()
    })
    .unwrap();
    assert_eq!(sent.bytes, 0);
    assert_eq!(sent.write_ms, 0.0);
    assert!(sink.bytes.is_empty());
    assert_eq!(sink.writes, 0);
    assert_eq!(sink.flushes, 0);
}

#[test]
fn one_large_write_is_streamed_in_bounded_underlying_writes() {
    let mut sink = Sink::default();
    let payload = vec![b'x'; CAPACITY * 5 + 13];
    let sent = frame(&mut sink, |out| out.write_all(&payload)).unwrap();
    assert!(sink.largest <= CAPACITY);
    assert_eq!(sent.bytes, payload.len() + BEGIN.len() + END.len());
    assert_eq!(
        &sink.bytes[BEGIN.len()..sink.bytes.len() - END.len()],
        payload
    );
}

#[test]
fn partial_underlying_writes_are_completed_without_reordering_or_miscounting() {
    let mut sink = Sink {
        partial: 3,
        ..Sink::default()
    };
    let sent = frame(&mut sink, |out| out.write_all(b"abcdefghijk")).unwrap();
    assert_eq!(sink.bytes, [BEGIN, b"abcdefghijk", END].concat());
    assert_eq!(sent.bytes, sink.bytes.len());
}

#[test]
fn a_render_failure_discards_buffered_payload_and_preserves_the_first_error() {
    let mut sink = Sink::default();
    let error = frame(&mut sink, |out| {
        out.write_all(b"must not escape")?;
        Err(io::Error::other("render failed first"))
    })
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "render failed first");
    assert_eq!(sink.bytes, [BEGIN, APC_END, END].concat());
    assert!(!sink.bytes.windows(4).any(|part| part == b"must"));
}

#[test]
fn an_ignored_partial_io_error_never_turns_into_a_success_or_replays_the_buffer() {
    // BEGIN succeeds; the large payload writes three bytes and then fails.
    let mut sink = Sink {
        partial: 3,
        fail_write: Some(5),
        ..Sink::default()
    };
    let error = frame(&mut sink, |out| {
        let payload = vec![b'p'; CAPACITY + 1];
        let _ = out.write_all(&payload);
        assert!(out.write(b"later body").is_err());
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(error.raw_os_error(), Some(libc::EIO));
    assert_eq!(sink.bytes, [BEGIN, b"ppp", APC_END, END].concat());
}

#[test]
fn flush_failure_remains_an_error_and_cleanup_does_not_repeat_payload() {
    let mut sink = Sink {
        fail_flush: true,
        ..Sink::default()
    };
    let error = frame(&mut sink, |out| out.write_all(b"only once"))
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "first flush failed");
    assert_eq!(occurrences(&sink.bytes, b"only once"), 1);
    assert!(sink.bytes.ends_with(&[APC_END, END].concat()));
}

#[test]
fn failure_while_sending_begin_is_reported_and_still_attempts_cleanup() {
    let mut sink = Sink {
        partial: 3,
        fail_write: Some(2),
        ..Sink::default()
    };
    let error = frame(&mut sink, |out| out.write_all(b"never buffered"))
        .err()
        .unwrap();
    assert_eq!(error.raw_os_error(), Some(libc::EIO));
    assert_eq!(sink.bytes, [&BEGIN[..3], APC_END, END].concat());
}

#[test]
fn panic_discards_buffered_uploads_instead_of_using_bufwriters_drop_flush() {
    let mut sink = Sink::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = frame(&mut sink, |out| {
            out.write_all(b"not complete")?;
            panic!("renderer panic")
        });
    }));
    assert!(result.is_err());
    assert_eq!(sink.bytes, [BEGIN, APC_END, END].concat());
}

#[test]
fn the_original_io_failure_wins_over_a_later_renderer_failure() {
    let mut sink = Sink {
        fail_write: Some(1),
        ..Sink::default()
    };
    let error = frame(&mut sink, |out| {
        assert_eq!(
            out.write(b"body").unwrap_err().raw_os_error(),
            Some(libc::EIO)
        );
        Err(io::Error::other("later render failure"))
    })
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EIO));
}

#[test]
fn a_cleanup_io_failure_never_replaces_the_renderer_error() {
    let mut sink = Sink {
        fail_write: Some(2),
        ..Sink::default()
    };
    let error = frame(&mut sink, |out| {
        out.write_all(b"discard")?;
        Err(io::Error::other("renderer failed first"))
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "renderer failed first");
    assert!(!sink.bytes.windows(7).any(|value| value == b"discard"));
}

#[test]
fn interrupted_large_writes_retry_then_complete_partial_writes_exactly_once() {
    let mut sink = Sink {
        partial: 8192,
        interrupt_write: Some(2),
        ..Sink::default()
    };
    let payload = vec![b'i'; CAPACITY * 2 + 17];
    let sent = frame(&mut sink, |out| out.write_all(&payload)).unwrap();
    assert_eq!(sink.bytes, [BEGIN, payload.as_slice(), END].concat());
    assert_eq!(sent.bytes, sink.bytes.len());
    assert!(sink.largest <= CAPACITY);
    assert_eq!(occurrences(&sink.bytes, BEGIN), 1);
    assert_eq!(occurrences(&sink.bytes, END), 1);
}

#[test]
fn an_interrupted_final_flush_retries_without_repeating_the_frame() {
    let mut sink = Sink {
        interrupt_flush: true,
        ..Sink::default()
    };
    let sent = frame(&mut sink, |out| out.write_all(b"one frame")).unwrap();
    assert_eq!(sink.bytes, [BEGIN, b"one frame", END].concat());
    assert_eq!(sent.bytes, sink.bytes.len());
    assert_eq!(sink.flushes, 2);
}
