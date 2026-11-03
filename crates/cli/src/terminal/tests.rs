use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

mod focus;
mod frame_health;
mod scroll;
mod view;

#[tokio::test]
async fn mouse_motion_is_coalesced_while_input_is_blocked_without_releasing_control() {
    let states = states();
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    session.lease = Some("held".into());
    session.pending = true;
    for x in 0..1000 {
        session.command(
            Command::Input {
                frame: "frame".into(),
                action: json!({"type":"move","x":x,"y":10}),
            },
            &states,
        );
    }
    assert_eq!(session.queue.len(), 1);
    assert_eq!(session.queue[0].1["x"], 999);
    assert!(!session.releasing);
    assert!(session.operations.is_empty());
    session.command(
        Command::Input {
            frame: "frame".into(),
            action: json!({"type":"pointer","phase":"down","button":"left","x":999,"y":10}),
        },
        &states,
    );
    for x in 0..1000 {
        session.command(
            Command::Input {
                frame: "frame".into(),
                action: json!({"type":"pointer","phase":"move","button":"left","x":x,"y":10}),
            },
            &states,
        );
    }
    session.command(
        Command::Input {
            frame: "frame".into(),
            action: json!({"type":"pointer","phase":"up","button":"left","x":999,"y":10}),
        },
        &states,
    );
    assert_eq!(session.queue.len(), 4);
    assert_eq!(session.queue[1].1["phase"], "down");
    assert_eq!(session.queue[2].1["phase"], "move");
    assert_eq!(session.queue[3].1["phase"], "up");
    assert!(!session.releasing);
}

#[tokio::test]
async fn queued_typing_is_batched_in_order_without_combining_different_frame_keys() {
    use axum::{Json, Router, routing::post};
    let (sent, mut received) = mpsc::channel(4);
    let router = Router::new().route(
        "/input",
        post(move |Json(body): Json<Value>| {
            let sent = sent.clone();
            async move {
                sent.send(body).await.unwrap();
                Json(json!({"ok":true}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut session = Session::new(format!("http://{}/", listener.local_addr().unwrap()), false);
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let states = states();
    session.lease = Some("held".into());
    session.pending = true;
    for index in 0..20 {
        session.command(
            Command::Input {
                frame: if index < 17 { "first" } else { "second" }.into(),
                action: json!({"type":"type","text":char::from(b'a' + index).to_string()}),
            },
            &states,
        );
    }
    session.pending = false;
    session.send_next();
    for (expected_frame, expected) in [("first", 0..16), ("first", 16..17), ("second", 17..20)] {
        let body = tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(body["frame"], expected_frame);
        let texts: Vec<_> = body["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|action| action["text"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            texts.concat(),
            expected
                .map(|index| char::from(b'a' + index))
                .collect::<String>()
        );
        assert_eq!(
            texts.len(),
            1,
            "Adjacent plain text shares one native type action"
        );
        let result = session.operations.join_next().await.unwrap().unwrap();
        session.completed(result, &states);
    }
    assert!(session.queue.is_empty());
    assert!(!session.pending);
    server.abort();
}

#[tokio::test]
async fn release_completion_preserves_the_error_that_ended_control() {
    let states = states();
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    session.completed(
        (
            Operation::Input,
            Err(Error::new(Code::StaleFrame, "Wait for a fresh frame")),
        ),
        &states,
    );
    let error = states.borrow().message.clone();
    assert!(error.contains("stale-frame"));
    session.completed((Operation::Release, Ok(json!({"ok":true}))), &states);
    assert_eq!(states.borrow().message, error);
    session.pending = true;
    session.command(
        Command::Suspend("Control released: terminal lost focus"),
        &states,
    );
    session.completed((Operation::Release, Ok(json!({"ok":true}))), &states);
    assert!(states.borrow().message.contains("lost focus"));
}

#[test]
fn watching_accepts_plain_take_control_and_quit_keys_without_a_prefix() {
    let key = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
    assert_eq!(
        input::shortcut(key('t'), false, false, Some("shown")),
        Some(Command::Take("shown".into()))
    );
    assert_eq!(
        input::shortcut(key('q'), false, false, Some("shown")),
        Some(Command::Quit)
    );
    assert_eq!(input::shortcut(key('t'), false, true, Some("shown")), None);
    assert_eq!(input::shortcut(key('t'), false, false, None), None);
    assert_eq!(
        input::shortcut(key('q'), false, true, None),
        Some(Command::Quit)
    );
}

#[test]
fn control_chords_release_or_exit_but_plain_typing_keeps_going_to_the_helper() {
    let key = |c, modifiers| KeyEvent::new(KeyCode::Char(c), modifiers);
    assert_eq!(
        input::shortcut(key('g', KeyModifiers::CONTROL), true, false, None),
        Some(Command::GiveBack)
    );
    assert_eq!(
        input::shortcut(key('q', KeyModifiers::CONTROL), true, false, None),
        Some(Command::Quit)
    );
    for character in ['q', 't', 's', 'g', 'c'] {
        let event = key(character, KeyModifiers::NONE);
        assert_eq!(input::shortcut(event, true, false, Some("shown")), None);
        assert_eq!(
            input::key(event),
            Some(json!({"type":"type","text":character.to_string()}))
        );
    }
}

#[test]
fn frames_fit_the_pane_without_turning_the_footer_into_remote_input() {
    let fit = graphics::Layout::fit(100, 32, (1000, 640), (1600, 900)).unwrap();
    assert_eq!(
        fit,
        graphics::Layout {
            columns: 100,
            rows: 28
        }
    );
    let point = fit.point(0, 0, (1600.0, 900.0)).unwrap();
    assert_eq!(point.0, 8.0);
    assert!((point.1 - 900.0 / 56.0).abs() < 1e-10);
    assert!(fit.point(100, 1, (1600.0, 900.0)).is_none());
    assert!(fit.point(1, 28, (1600.0, 900.0)).is_none());
    assert!(graphics::Layout::fit(100, 2, (1000, 40), (1600, 900)).is_none());
    assert!(graphics::Layout::fit(100, 32, (0, 0), (1600, 900)).is_none());
    assert!(fit.point(1, 1, (f64::NAN, 900.0)).is_none());
}

#[test]
fn graphics_queries_require_a_complete_reply_with_the_matching_image_id() {
    assert!(graphics::supported(
        format!("noise\x1b_Gi={};OK\x1b\\", graphics::QUERY).as_bytes()
    ));
    assert!(!graphics::supported(b"\x1b_Gi=1;OK\x1b\\"));
    assert!(!graphics::supported(
        format!("\x1b_Gi={};ENOENT\x1b\\", graphics::QUERY).as_bytes()
    ));
    assert!(!graphics::supported(
        format!("\x1b_Gi={};OK", graphics::QUERY).as_bytes()
    ));
}

#[test]
fn large_images_finish_uploading_before_their_placement_is_created() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let png = vec![42; 15000];
    let mut wire = Vec::new();
    graphics::Painter::default()
        .draw(
            &mut wire,
            &png,
            graphics::Layout {
                columns: 80,
                rows: 20,
            },
        )
        .unwrap();
    let wire = String::from_utf8(wire).unwrap();
    assert!(wire.starts_with("\x1b_Ga=t,f=100,t=d,i=47471,q=2,m=1;"));
    let mut payload = String::new();
    let mut parts = wire
        .split("\x1b_G")
        .skip(1)
        .filter_map(|part| part.split_once("\x1b\\").unwrap().0.split_once(';'))
        .peekable();
    while let Some((head, body)) = parts.next() {
        assert!(body.len() <= 4096);
        assert!(head.ends_with(if parts.peek().is_some() { "m=1" } else { "m=0" }));
        payload.push_str(body);
    }
    assert_eq!(STANDARD.decode(payload).unwrap(), png);
    assert!(wire.ends_with("\x1b_Ga=p,i=47471,p=1,c=80,r=20,C=1,q=2\x1b\\\x1b[?2026l"));
}

#[test]
fn new_frames_are_placed_before_the_visible_image_is_deleted_and_memory_stays_bounded() {
    let layout = graphics::Layout {
        columns: 80,
        rows: 20,
    };
    let mut painter = graphics::Painter::default();
    for index in 0..6 {
        let mut wire = Vec::new();
        painter.draw(&mut wire, &[index], layout).unwrap();
        let wire = String::from_utf8(wire).unwrap();
        let image = graphics::IMAGE + if index.is_multiple_of(2) { 0 } else { 2 };
        assert!(wire.starts_with(&format!("\x1b_Ga=t,f=100,t=d,i={image},q=2,m=0;")));
        let place = wire.find(&format!("\x1b_Ga=p,i={image},")).unwrap();
        if index > 0 {
            let previous = graphics::IMAGE + if index.is_multiple_of(2) { 2 } else { 0 };
            let remove = wire.find(&format!("\x1b_Ga=d,d=I,i={previous},")).unwrap();
            assert!(
                place < remove,
                "The visible image must survive until its replacement is placed"
            );
            assert!(!wire.contains(&format!("\x1b_Ga=t,f=100,t=d,i={previous},")));
        } else {
            assert!(!wire.contains("a=d"));
        }
    }
}

#[test]
fn unchanged_pixels_do_not_reupload_and_resizing_only_moves_the_existing_placement() {
    let mut painter = graphics::Painter::default();
    let layout = graphics::Layout {
        columns: 80,
        rows: 20,
    };
    painter.draw(&mut Vec::new(), b"unchanged", layout).unwrap();
    let mut wire = Vec::new();
    painter.draw(&mut wire, b"unchanged", layout).unwrap();
    assert!(wire.is_empty());
    painter
        .draw(
            &mut wire,
            b"unchanged",
            graphics::Layout {
                columns: 60,
                rows: 15,
            },
        )
        .unwrap();
    let wire = String::from_utf8(wire).unwrap();
    assert!(wire.contains("a=p,i=47471,p=1,c=60,r=15"));
    assert!(!wire.contains("a=t"));
    assert!(!wire.contains("a=d"));
    let mut clear = Vec::new();
    painter.clear(&mut clear).unwrap();
    assert!(!clear.is_empty());
    clear.clear();
    painter.clear(&mut clear).unwrap();
    assert!(clear.is_empty());
}

#[test]
fn typing_keeps_unicode_and_shortcuts_keep_their_modifiers() {
    assert_eq!(
        input::key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
        Some(json!({"type":"key","key":"up"}))
    );
    assert_eq!(
        input::key(KeyEvent::new(KeyCode::Char('ა'), KeyModifiers::NONE)),
        Some(json!({"type":"type","text":"ა"}))
    );
    assert_eq!(
        input::key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::SUPER)),
        Some(json!({"type":"key","key":"cmd+c"}))
    );
    assert_eq!(
        input::key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(json!({"type":"key","key":"shift+tab"}))
    );
    assert!(input::key(KeyEvent::new(KeyCode::F(99), KeyModifiers::NONE)).is_none());
    assert!(input::key(KeyEvent::new(KeyCode::F(13), KeyModifiers::NONE)).is_none());
    for code in [
        KeyCode::Char('['),
        KeyCode::F(12),
        KeyCode::Delete,
        KeyCode::BackTab,
    ] {
        let action = input::key(KeyEvent::new(code, KeyModifiers::CONTROL)).unwrap();
        assert!(atlas_core::actions::valid_key(
            action["key"].as_str().unwrap()
        ));
    }
}

#[test]
fn mouse_phases_and_scroll_directions_match_native_input() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    for (kind, field, expected) in [
        (MouseEventKind::ScrollUp, "dy", json!(-1)),
        (MouseEventKind::ScrollDown, "dy", json!(1)),
        (MouseEventKind::ScrollLeft, "dx", json!(-1)),
        (MouseEventKind::ScrollRight, "dx", json!(1)),
        (
            MouseEventKind::Down(MouseButton::Left),
            "phase",
            json!("down"),
        ),
        (
            MouseEventKind::Drag(MouseButton::Left),
            "phase",
            json!("move"),
        ),
        (MouseEventKind::Up(MouseButton::Left), "phase", json!("up")),
    ] {
        let event = MouseEvent {
            kind,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        };
        let action = input::mouse(event, (12.0, 30.0)).unwrap();
        assert_eq!(action[field], expected);
        atlas_core::actions::check(
            &serde_json::from_value::<Vec<atlas_core::actions::Action>>(json!([action])).unwrap(),
            &atlas_core::session::Display {
                width: 1600.0,
                height: 900.0,
                scale: 2.0,
            },
        )
        .unwrap();
    }
}

fn states() -> watch::Sender<State> {
    watch::channel(State {
        revision: 0,
        frame: Some(Arc::new(SharedFrame {
            png: vec![],
            rgb: None,
            pixels: (0, 0),
            metrics: None,
            key: "frame".into(),
            display: (1600.0, 900.0),
        })),
        controlling: false,
        paused: false,
        phase: ControlPhase::Watching,
        view: viewport::Viewport::default(),
        diagnostics: 0,
        input_ack_ms: None,
        last_frame_error: None,
        message: String::new(),
    })
    .0
}

#[tokio::test]
async fn observation_drops_input_and_read_only_sessions_never_acquire_control() {
    let states = states();
    let mut session = Session::new("http://127.0.0.1:1/".into(), true);
    session.command(Command::Take("frame".into()), &states);
    session.command(Command::Stop, &states);
    session.command(
        Command::Input {
            frame: "frame".into(),
            action: json!({"type":"type","text":"hello"}),
        },
        &states,
    );
    assert!(session.operations.is_empty());
    assert!(session.queue.is_empty());
}

#[tokio::test]
async fn giving_back_during_acquisition_releases_the_lease_when_it_arrives() {
    let states = states();
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    session.pending = true;
    session.release(&states);
    session.completed((Operation::Acquire, Ok(json!({"lease":"later"}))), &states);
    assert!(!states.borrow().controlling);
    assert!(session.releasing);
    assert_eq!(session.lease.as_deref(), Some("later"));
    session.operations.abort_all();
    while session.operations.join_next().await.is_some() {}
}

#[tokio::test]
async fn renewal_and_cancellation_reach_the_service_before_blocked_input_finishes() {
    use axum::{Json, Router, routing::post};
    use tokio::sync::Notify;
    let input_started = Arc::new(Notify::new());
    let released = Arc::new(Notify::new());
    let start = input_started.clone();
    let release_input = released.clone();
    let release = released.clone();
    let renewed = Arc::new(Notify::new());
    let notify_renewed = renewed.clone();
    let router = Router::new()
        .route(
            "/control/renew",
            post(move || async move {
                notify_renewed.notify_one();
                Json(json!({"ok":true}))
            }),
        )
        .route(
            "/input",
            post(move || async move {
                start.notify_one();
                release_input.notified().await;
                Json(json!({"ok":true}))
            }),
        )
        .route(
            "/control/release",
            post(move || async move {
                release.notify_one();
                Json(json!({"ok":true}))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let states = states();
    let mut session = Session::new(format!("http://{address}/"), false);
    session.lease = Some("owned".into());
    session.command(
        Command::Input {
            frame: "frame".into(),
            action: json!({"type":"type", "text":"hello"}),
        },
        &states,
    );
    tokio::time::timeout(Duration::from_secs(2), input_started.notified())
        .await
        .unwrap();
    session.renew(&states);
    tokio::time::timeout(Duration::from_secs(2), renewed.notified())
        .await
        .unwrap();
    let result = session.operations.join_next().await.unwrap().unwrap();
    assert_eq!(result.0, Operation::Renew);
    session.completed(result, &states);
    assert!(
        session.pending,
        "Renewal must not clear the input still in progress"
    );
    assert!(!session.renewing);
    session.release(&states);
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(result) = session.operations.join_next().await {
            session.completed(result.unwrap(), &states);
        }
    })
    .await
    .unwrap();
    assert!(session.lease.is_none());
    assert!(!session.pending);
    assert!(!states.borrow().controlling);
    server.abort();
}

#[tokio::test]
async fn unconfirmed_cleanup_prevents_reacquiring_control() {
    let states = states();
    let mut session = Session::new("http://127.0.0.1:1/".into(), false);
    session.lease = Some("owned".into());
    session.completed(
        (
            Operation::Release,
            Err(Error::new(Code::Uncertain, "cleanup unknown").uncertain()),
        ),
        &states,
    );
    session.command(Command::Take("frame".into()), &states);
    assert!(session.cleanup_error.is_some());
    assert!(session.operations.is_empty());
}

fn encoded_picture(picture: &image::RgbImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(picture.clone())
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn tile_uploads(wire: &[u8]) -> Vec<(String, Vec<u8>)> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::io::Read;
    let wire = std::str::from_utf8(wire).unwrap();
    let mut uploads = Vec::new();
    let mut header = String::new();
    let mut payload = String::new();
    for part in wire.split("\x1b_G").skip(1) {
        let part = part.split_once("\x1b\\").unwrap().0;
        let Some((control, data)) = part.split_once(';') else {
            continue;
        };
        if control.starts_with("a=t,") || (control.starts_with("a=f,") && control.contains(",s=")) {
            header = control.into();
            payload.clear();
        }
        payload.push_str(data);
        if control.ends_with("m=0") {
            let compressed = STANDARD.decode(&payload).unwrap();
            let mut pixels = Vec::new();
            flate2::read::ZlibDecoder::new(compressed.as_slice())
                .read_to_end(&mut pixels)
                .unwrap();
            uploads.push((header.clone(), pixels));
        }
    }
    uploads
}

#[test]
fn png_frames_round_trip_the_canvas_and_patch_small_changes_when_supported() {
    let mut picture = image::RgbImage::from_fn(512, 256, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 251) as u8, ((x + y) % 251) as u8])
    });
    let layout = graphics::Layout {
        columns: 64,
        rows: 16,
    };
    let geometry = (64, 16, 512, 256);
    let mut painter = tiles::Tiles::default();
    painter.set_patches(true);
    let mut wire = Vec::new();
    painter
        .draw(&mut wire, &encoded_picture(&picture), layout, geometry)
        .unwrap();
    let uploads = tile_uploads(&wire);
    assert_eq!(uploads.len(), 1);
    assert!(uploads[0].0.contains("f=24,s=512,v=256,o=z"));
    assert_eq!(uploads[0].1, *picture.as_raw());
    wire.clear();
    assert_eq!(
        painter
            .draw(&mut wire, &encoded_picture(&picture), layout, geometry)
            .unwrap(),
        0
    );
    assert!(wire.is_empty());
    picture.put_pixel(10, 10, image::Rgb([255, 0, 255]));
    painter
        .draw(&mut wire, &encoded_picture(&picture), layout, geometry)
        .unwrap();
    let updated = tile_uploads(&wire);
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].1, [255, 0, 255]);
    assert!(updated[0].0.contains("x=10,y=10,f=24,s=1,v=1,"));
    wire.clear();
    painter.clear(&mut wire).unwrap();
    assert_eq!(
        std::str::from_utf8(&wire).unwrap().matches("a=d").count(),
        2
    );
}

#[test]
fn terminal_tiles_preserve_capture_resolution_for_the_terminals_final_fit() {
    let picture = image::RgbImage::from_pixel(512, 256, image::Rgb([40, 60, 80]));
    let mut wire = Vec::new();
    tiles::Tiles::default()
        .draw(
            &mut wire,
            &encoded_picture(&picture),
            graphics::Layout {
                columns: 64,
                rows: 16,
            },
            (64, 16, 256, 128),
        )
        .unwrap();
    let uploads = tile_uploads(&wire);
    assert_eq!(uploads.len(), 1);
    assert_eq!(
        uploads
            .iter()
            .map(|(_, pixels)| pixels.len())
            .sum::<usize>(),
        512 * 256 * 3
    );
    assert!(
        uploads
            .iter()
            .all(|(header, _)| header.contains("s=512,v=256"))
    );
}

#[test]
#[ignore = "Run explicitly in release mode to measure terminal encoding on a saved helper frame"]
fn terminal_rendering_benchmark() {
    use std::time::Instant;
    let png = std::env::var_os("ATLAS_TERMINAL_BENCH_IMAGE")
        .map(|path| std::fs::read(path).unwrap())
        .unwrap_or_else(|| {
            encoded_picture(&image::RgbImage::from_fn(1600, 1040, |x, y| {
                image::Rgb([
                    ((x * 31 + y * 17) % 256) as u8,
                    ((x * 7 + y * 73) % 256) as u8,
                    ((x * 19 + y * 3) % 256) as u8,
                ])
            }))
        });
    let mut picture = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .unwrap()
        .to_rgb8();
    let layout = graphics::Layout::fit(120, 42, (960, 672), picture.dimensions()).unwrap();
    let mut old = Vec::new();
    graphics::Painter::default()
        .draw(&mut old, &png, layout)
        .unwrap();
    let mut renderer = tiles::Tiles::default();
    renderer.set_patches(true);
    let mut wire = Vec::new();
    let start = Instant::now();
    renderer
        .draw(&mut wire, &png, layout, (120, 42, 960, 672))
        .unwrap();
    let initial_ms = start.elapsed().as_secs_f64() * 1000.0;
    let full = wire.len();
    for y in picture.height() / 2..picture.height() / 2 + 16 {
        for x in picture.width() / 2..picture.width() / 2 + 16 {
            picture.put_pixel(x, y, image::Rgb([255, 0, 0]));
        }
    }
    let changed = encoded_picture(&picture);
    wire.clear();
    let start = Instant::now();
    renderer
        .draw(&mut wire, &changed, layout, (120, 42, 960, 672))
        .unwrap();
    let update_ms = start.elapsed().as_secs_f64() * 1000.0;
    let update = wire.len();
    let count = tile_uploads(&wire).len();
    wire.clear();
    let start = Instant::now();
    renderer
        .draw(&mut wire, &changed, layout, (120, 42, 960, 672))
        .unwrap();
    let unchanged_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "terminal benchmark: old_full_bytes={} tiled_full_bytes={full} small_change_bytes={update} changed_tiles={count} initial_ms={initial_ms:.2} update_ms={update_ms:.2} unchanged_ms={unchanged_ms:.2}",
        old.len()
    );
    assert!(
        update < full / 4,
        "A small change must not upload the whole picture"
    );
    assert!(wire.is_empty());
}
