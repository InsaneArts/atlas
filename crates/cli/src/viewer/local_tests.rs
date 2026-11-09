use super::*;

#[test]
fn a_frames_key_and_display_size_are_read_from_its_headers() {
    let reply = b"HTTP/1.1 200 OK\r\nX-Frame-Id: abc123\r\nx-display-width: 1440\r\nContent-Length: 2\r\n\r\nok";
    assert_eq!(reply_header(reply, "x-frame-id").as_deref(), Some("abc123"));
    assert_eq!(
        reply_header(reply, "X-Display-Width").as_deref(),
        Some("1440"),
        "named in any case"
    );
    assert_eq!(reply_header(reply, "x-display-height"), None);
    assert_eq!(
        reply_header(b"HTTP/1.1 200 OK\r\nX-Frame-Id: a", "x-frame-id"),
        None,
        "not until the head has ended"
    );
}

#[test]
fn replies_are_read_by_their_framing_and_status() {
    let plain = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 11\r\n\r\n{\"ok\":true}";
    assert_eq!(
        parse_response(plain),
        Some((200, b"{\"ok\":true}".to_vec()))
    );
    assert_eq!(
        parse_response(&plain[..plain.len() - 3]),
        None,
        "not all here yet"
    );

    let chunked = b"HTTP/1.1 409 Conflict\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"ok\r\n8\r\n\":false}\r\n0\r\n\r\n";
    assert_eq!(
        parse_response(chunked),
        Some((409, b"{\"ok\":false}".to_vec()))
    );
    assert_eq!(parse_response(b"HTTP/1.1 200 OK\r\n\r\n"), None, "unframed");
    assert_eq!(parse_response(b"garbage\r\n\r\n"), None);
}
struct LocalReply {
    url: String,
    stop: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
    holding: std::sync::mpsc::Receiver<()>,
    closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl LocalReply {
    fn serving(fragments: Vec<(Duration, Vec<u8>)>, hold_open: bool) -> Self {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/private/", listener.local_addr().unwrap());
        let (stop, stopped) = std::sync::mpsc::channel();
        let (held, holding) = std::sync::mpsc::channel();
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished = std::sync::Arc::clone(&closed);
        let thread = std::thread::spawn(move || {
            let result = || -> std::io::Result<()> {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if stopped.recv_timeout(Duration::from_millis(2)).is_ok() {
                                return Ok(());
                            }
                        }
                        Err(error) => return Err(error),
                    }
                };
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_millis(500)))?;
                stream.set_write_timeout(Some(Duration::from_millis(500)))?;
                let mut request = Vec::new();
                let mut bytes = [0; 512];
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let read = stream.read(&mut bytes)?;
                    if read == 0 || request.len() + read > 4096 {
                        return Ok(());
                    }
                    request.extend_from_slice(&bytes[..read]);
                }
                for (delay, bytes) in fragments {
                    if stopped.recv_timeout(delay).is_ok() {
                        return Ok(());
                    }
                    stream.write_all(&bytes)?;
                }
                if hold_open {
                    // The fixture, not a guessed duration, owns connection closure.
                    // Drop always releases this hold before joining the server.
                    // A fixture dropped before observation need not hear readiness.
                    let _ = held.send(());
                    let _ = stopped.recv();
                }
                Ok(())
            };
            let _ = result();
            finished.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        Self {
            url,
            stop,
            thread: Some(thread),
            holding,
            closed,
        }
    }

    fn whole(bytes: &[u8]) -> Self {
        Self::serving(vec![(Duration::ZERO, bytes.to_vec())], false)
    }
}

impl Drop for LocalReply {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[test]
fn framing_keeps_its_existing_header_precedence_and_permissive_chunks() {
    let cases = [
        (
            "Content-Length: 2\r\nContent-Length: 3",
            "abcTAIL",
            Some("abc"),
        ),
        ("Content-Length: bad\r\nContent-Length: 3", "abc", None),
        ("Content-Length: 3\r\nContent-Length: bad", "abc", None),
        (
            "Content-Length: bad\r\nTransfer-Encoding: chunked",
            "0\r\n",
            None,
        ),
        (
            "Transfer-Encoding: chunked\r\nContent-Length: bad",
            "0\r\n",
            None,
        ),
        (
            "Content-Length: 99\r\nTransfer-Encoding: CHUNKED",
            "1;ext=yes\r\naXY0\r\n",
            Some("a"),
        ),
        (
            "Transfer-Encoding: chunked\r\nTransfer-Encoding: identity\r\nContent-Length: 1",
            "a",
            Some("a"),
        ),
        (
            "Transfer-Encoding: identity\r\nTransfer-Encoding: chunked",
            "0\r\n",
            Some(""),
        ),
        ("Transfer-Encoding: gzip, chunked", "0\r\n", None),
        (
            "Transfer-Encoding: chunked",
            "1\r\na\r\n0\r\nTRAILERS",
            Some("a"),
        ),
    ];
    for (headers, body, expected) in cases {
        let wire = format!("HTTP/1.1 200 OK\r\n{headers}\r\n\r\n{body}");
        assert_eq!(
            parse_response(wire.as_bytes()),
            expected.map(|body| (200, body.as_bytes().to_vec())),
            "{headers:?}"
        );
    }
    for (status, expected) in [
        ("HTTP/1.1 200anything", Some(200)),
        ("HTTP/1.0 200 OK", None),
        ("HTTP/1.1 20 OK", None),
        ("HTTP/1.1 two OK", None),
    ] {
        let wire = format!("{status}\r\nContent-Length: 0\r\n\r\n");
        assert_eq!(
            parse_response(wire.as_bytes()).map(|(status, _)| status),
            expected
        );
    }
}

#[test]
fn frame_header_lookup_keeps_the_first_duplicate_and_trimmed_value() {
    let reply = b"HTTP/1.1 200 OK\r\nX-FrAmE-Id: \t first \t\r\nx-frame-id: second\r\nX-Display-Width: NaN\r\nContent-Length: 0\r\n\r\n";
    assert_eq!(reply_header(reply, "x-frame-id").as_deref(), Some("first"));
    assert_eq!(
        reply_header(reply, "X-DISPLAY-WIDTH").as_deref(),
        Some("NaN")
    );
}

#[test]
fn fragmented_plain_and_chunked_frames_keep_their_headers_and_body() {
    let head =
        "HTTP/1.1 200 OK\r\nX-Frame-Id: key\r\nX-Display-Width: 1440\r\nX-Display-Height: 900\r\n";
    for framing in [
        "Content-Length: 3\r\n\r\npng",
        "Transfer-Encoding: chunked\r\n\r\n1;x=y\r\np\r\n2\r\nng\r\n0\r\n",
    ] {
        let wire = format!("{head}{framing}").into_bytes();
        for end in 0..wire.len() {
            assert_eq!(parse_response(&wire[..end]), None, "prefix {end}");
        }
        assert_eq!(parse_response(&wire), Some((200, b"png".to_vec())));
        let fragments = wire
            .chunks(7)
            .map(|part| (Duration::from_millis(2), part.to_vec()))
            .collect();
        let server = LocalReply::serving(fragments, false);
        let frame = fetch_frame(&server.url, Duration::from_secs(2), wire.len()).unwrap();
        assert_eq!(frame.png, b"png");
        assert_eq!(frame.key, "key");
        assert_eq!(frame.display, (1440.0, 900.0));
    }
}

#[test]
fn the_wire_limit_counts_the_head_and_accepts_only_the_exact_complete_size() {
    let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\npng";
    for (limit, accepted) in [(wire.len(), true), (wire.len() - 1, false), (3, false)] {
        let server = LocalReply::whole(wire);
        let reply = fetch(&server.url, "frame.png", Duration::from_secs(1), limit);
        if accepted {
            assert_eq!(reply.unwrap(), b"png");
        } else {
            let error = reply.unwrap_err();
            assert_eq!(
                (error.code, error.error),
                (unanswered().code, unanswered().error)
            );
        }
    }
}

#[test]
fn a_complete_body_returns_while_the_server_keeps_the_connection_open() {
    let (finished, watching) = std::sync::mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if watching.recv_timeout(Duration::from_secs(20))
            == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        {
            std::process::abort();
        }
    });
    let server = LocalReply::serving(
        vec![(
            Duration::ZERO,
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec(),
        )],
        true,
    );
    assert_eq!(
        fetch(&server.url, "", Duration::from_millis(500), 1024).unwrap(),
        b"ok"
    );
    server.holding.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(!server.closed.load(std::sync::atomic::Ordering::SeqCst));
    drop(server);
    finished.send(()).unwrap();
    watchdog.join().unwrap();
}

#[test]
fn slow_fragments_share_one_overall_deadline() {
    let server = LocalReply::serving(
        vec![
            (
                Duration::ZERO,
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n".to_vec(),
            ),
            (Duration::from_millis(90), b"o".to_vec()),
            (Duration::from_millis(90), b"k".to_vec()),
        ],
        false,
    );
    let error = fetch(&server.url, "", Duration::from_millis(140), 1024).unwrap_err();
    assert_eq!(
        (error.code, error.error),
        (unanswered().code, unanswered().error)
    );
}

#[test]
fn incomplete_or_malformed_replies_at_eof_keep_the_generic_error() {
    for wire in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 1".as_slice(),
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nx",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nx",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nx\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: bad\r\n\r\nx",
        b"not http\r\n\r\n",
    ] {
        let server = LocalReply::whole(wire);
        let error = fetch(&server.url, "", Duration::from_secs(1), 1024).unwrap_err();
        assert_eq!(
            (error.code, error.error),
            (unanswered().code, unanswered().error)
        );
    }
}

#[test]
fn frame_metadata_keeps_its_existing_missing_and_numeric_rules() {
    for (headers, accepted) in [
        (
            "X-Frame-Id: \r\nX-Display-Width: NaN\r\nX-Display-Height: inf",
            true,
        ),
        (
            "X-Frame-Id: k\r\nX-Display-Width: 0\r\nX-Display-Height: -2",
            true,
        ),
        ("X-Display-Width: 1\r\nX-Display-Height: 2", false),
        ("X-Frame-Id: k\r\nX-Display-Height: 2", false),
        ("X-Frame-Id: k\r\nX-Display-Width: 1", false),
        (
            "X-Frame-Id: k\r\nX-Display-Width: many\r\nX-Display-Height: 2",
            false,
        ),
    ] {
        let wire = format!("HTTP/1.1 200 OK\r\n{headers}\r\nContent-Length: 1\r\n\r\nx");
        let server = LocalReply::whole(wire.as_bytes());
        let frame = fetch_frame(&server.url, Duration::from_secs(1), 1024);
        if accepted {
            let frame = frame.unwrap();
            assert_eq!(frame.png, b"x");
            if frame.key.is_empty() {
                assert!(frame.display.0.is_nan());
                assert_eq!(frame.display.1, f64::INFINITY);
            } else {
                assert_eq!(frame.display, (0.0, -2.0));
            }
        } else {
            let error = frame.err().unwrap();
            assert_eq!(
                (error.code, error.error),
                (unanswered().code, unanswered().error)
            );
        }
    }
}
