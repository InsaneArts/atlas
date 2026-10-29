//! Discovery and bounded HTTP access to the local menu bar viewer.
use super::{MENUBAR_RECORD, SERVICE, unanswered};
use crate::error::{Code, Error, Result};
use serde_json::Value;
use std::{path::Path, time::Duration};

/// The viewer recorded at `record`, if it serves `port` and answers there
/// now as the very process the record names. A viewer from an older build
/// does not say its process; the record's counts then, when that process
/// runs Atlas. It blocks for up to a second: call it off the async runtime.
pub fn running(record: &Path, port: u16) -> Option<Value> {
    std::fs::read(record)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|running| {
            let url = running["url"].as_str().unwrap_or("");
            let recorded = running["pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok());
            if !url.starts_with(&format!("http://127.0.0.1:{port}/")) {
                return false;
            }

            match answering(url) {
                Some(Some(pid)) => recorded == Some(pid),
                Some(None) => recorded
                    .and_then(|pid| i32::try_from(pid).ok())
                    .is_some_and(crate::menubar::is_atlas),
                None => false,
            }
        })
}

/// Whether a viewer answers at `url` (its private address) right now, and
/// then the process it says serves it: None from a viewer built before
/// viewers said so.
pub(super) fn answering(url: &str) -> Option<Option<u32>> {
    let (status, body) = exchange(url, "GET", "", None, Duration::from_secs(1)).ok()?;
    let reply: Value = serde_json::from_slice(&body).ok()?;
    if status != 200 || reply["service"] != SERVICE {
        return None;
    }

    Some(
        reply["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok()),
    )
}

/// Whether the menu bar's viewer is recorded in `output` as serving `port`
/// and its process still runs Atlas, answering or not.
pub fn recorded(output: &Path, port: u16) -> bool {
    std::fs::read(output.join(MENUBAR_RECORD))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|record| {
            record["url"]
                .as_str()
                .is_some_and(|url| url.starts_with(&format!("http://127.0.0.1:{port}/")))
        })
        .and_then(|record| record["pid"].as_i64())
        .and_then(|pid| i32::try_from(pid).ok())
        .is_some_and(crate::menubar::is_atlas)
}

/// The private address of the menu bar's viewer, when it runs and answers
/// as the process its record names. It blocks for up to a second: call it
/// off the async runtime.
pub fn local(output: &Path, port: u16) -> Option<String> {
    running(&output.join(MENUBAR_RECORD), port)
        .and_then(|running| running["url"].as_str().map(str::to_owned))
}

/// What the local viewer at `url` serves at `route`, read within `deadline`
/// and at most `limit` bytes. Anything but a 200 is refused.
pub fn fetch(url: &str, route: &str, deadline: Duration, limit: usize) -> Result<Vec<u8>> {
    match exchange_within(url, "GET", route, None, deadline, limit)? {
        (200, body) => Ok(body),
        _ => Err(unanswered()),
    }
}

/// One request to the local viewer at `url`, as its own origin, within
/// `deadline` overall: the reply's status and body. Replies are small; a
/// larger one is refused.
pub(super) fn exchange(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    deadline: Duration,
) -> Result<(u16, Vec<u8>)> {
    exchange_within(url, method, route, body, deadline, 1 << 16)
}

/// [`exchange`], refusing a reply larger than `limit` bytes.
fn exchange_within(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    deadline: Duration,
    limit: usize,
) -> Result<(u16, Vec<u8>)> {
    let reply = exchange_raw(url, method, route, body, deadline, limit)?;
    Ok((reply.status, reply.body))
}

/// The menu bar viewer's newest frame, with the key control requests name
/// it by and the display's size in points, which input is given in.
pub(crate) struct SharedFrame {
    pub png: Vec<u8>,
    pub key: String,
    pub display: (f64, f64),
}

/// The local viewer's newest frame at `url`, read within `deadline` and at
/// most `limit` bytes.
pub(crate) fn fetch_frame(
    url: &str,
    deadline: Duration,
    limit: usize,
) -> Result<super::SharedFrame> {
    let reply = exchange_raw(url, "GET", "frame.png", None, deadline, limit)?;
    if reply.status != 200 {
        return Err(unanswered());
    }
    let named = |name: &str| reply_header(&reply.wire, name);
    let points = |name: &str| named(name).and_then(|value| value.parse::<f64>().ok());

    Ok(SharedFrame {
        png: reply.body,
        key: named("x-frame-id").ok_or_else(unanswered)?,
        display: (
            points("x-display-width").ok_or_else(unanswered)?,
            points("x-display-height").ok_or_else(unanswered)?,
        ),
    })
}

/// The value of the header `name` in a whole reply.
fn reply_header(reply: &[u8], name: &str) -> Option<String> {
    let split = reply.windows(4).position(|window| window == b"\r\n\r\n")?;
    std::str::from_utf8(&reply[..split])
        .ok()?
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim().to_owned())
}

/// A completed reply and its wire bytes, retained for frame header lookup.
struct Reply {
    wire: Vec<u8>,
    status: u16,
    body: Vec<u8>,
}

/// One request to the local viewer at `url`: its completed status and body,
/// with the whole wire reply retained for header lookup.
fn exchange_raw(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    deadline: Duration,
    limit: usize,
) -> Result<Reply> {
    use std::io::{Read, Write};

    // Any trouble on the wire reads the same to the person.
    let io = |_: std::io::Error| unanswered();

    let rest = url.strip_prefix("http://").ok_or_else(unanswered)?;
    let (authority, path) = rest.split_at(rest.find('/').ok_or_else(unanswered)?);
    let address: std::net::SocketAddr = authority.parse().map_err(|_| unanswered())?;

    let until = std::time::Instant::now() + deadline;
    let left = || {
        until
            .checked_duration_since(std::time::Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(unanswered)
    };

    let mut stream = std::net::TcpStream::connect_timeout(&address, left()?).map_err(io)?;
    stream.set_write_timeout(Some(left()?)).map_err(io)?;
    let mut head =
        format!("{method} {path}{route} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n");

    if let Some(body) = body {
        head.push_str(&format!(
            "Origin: http://{authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }

    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).map_err(io)?;

    if let Some(body) = body {
        stream.write_all(body).map_err(io)?;
    }

    let mut reply = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        stream.set_read_timeout(Some(left()?)).map_err(io)?;
        let read = stream.read(&mut chunk).map_err(io)?;
        if read == 0 {
            break;
        }

        reply.extend_from_slice(&chunk[..read]);

        if reply.len() > limit {
            return Err(unanswered());
        }

        if let Some((status, body)) = parse_response(&reply) {
            return Ok(Reply {
                wire: reply,
                status,
                body,
            });
        }
    }

    Err(unanswered())
}

/// A complete HTTP/1.1 reply's status and body, framed by Content-Length
/// or chunked encoding; None until all of it has arrived.
pub(super) fn parse_response(reply: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = reply.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&reply[..split]).ok()?;
    let rest = &reply[split + 4..];
    let mut lines = head.split("\r\n");
    let status = lines
        .next()?
        .strip_prefix("HTTP/1.1 ")?
        .get(..3)?
        .parse::<u16>()
        .ok()?;

    let mut length = None;
    let mut chunked = false;

    for line in lines {
        let (name, value) = line.split_once(':')?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.parse::<usize>().ok()?);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.eq_ignore_ascii_case("chunked");
        }
    }

    if chunked {
        let mut body = Vec::new();
        let mut at = rest;

        loop {
            let end = at.windows(2).position(|window| window == b"\r\n")?;
            let size = std::str::from_utf8(&at[..end])
                .ok()?
                .split(';')
                .next()?
                .trim();
            let size = usize::from_str_radix(size, 16).ok()?;
            at = &at[end + 2..];

            if size == 0 {
                return Some((status, body));
            }

            body.extend_from_slice(at.get(..size)?);
            at = at.get(size + 2..)?;
        }
    }

    let length = length?;
    Some((status, rest.get(..length)?.to_vec()))
}

/// POST `body` to `route` of the local viewer at `url`, as its own origin,
/// and read its JSON reply; a refusal is an error in its own words.
pub(crate) fn post_local(url: &str, route: &str, body: &Value) -> Result<Value> {
    let (status, reply) = exchange(
        url,
        "POST",
        route,
        Some(&serde_json::to_vec(body)?),
        Duration::from_secs(5),
    )?;
    let reply: Value = serde_json::from_slice(&reply).map_err(|_| unanswered())?;

    if status != 200 {
        // A refusal keeps its own code (busy, stale-frame…) when it is one
        // Atlas knows.
        let code = reply["code"]
            .as_str()
            .and_then(crate::error::Code::parse)
            .unwrap_or(Code::Menubar);
        let error = Error::new(
            code,
            reply["error"]
                .as_str()
                .unwrap_or("The menu bar's viewer refused")
                .to_owned(),
        );
        return Err(if reply["uncertain"] == true {
            error.uncertain()
        } else {
            error
        });
    }

    Ok(reply)
}

#[cfg(test)]
mod tests {
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
        closed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl LocalReply {
        fn serving(fragments: Vec<(Duration, Vec<u8>)>, hold: Duration) -> Self {
            use std::io::{Read, Write};

            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/private/", listener.local_addr().unwrap());
            let (stop, stopped) = std::sync::mpsc::channel();
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
                    let _ = stopped.recv_timeout(hold);
                    Ok(())
                };
                let _ = result();
                finished.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            Self {
                url,
                stop,
                thread: Some(thread),
                closed,
            }
        }

        fn whole(bytes: &[u8]) -> Self {
            Self::serving(vec![(Duration::ZERO, bytes.to_vec())], Duration::ZERO)
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
        let head = "HTTP/1.1 200 OK\r\nX-Frame-Id: key\r\nX-Display-Width: 1440\r\nX-Display-Height: 900\r\n";
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
            let server = LocalReply::serving(fragments, Duration::ZERO);
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
        let server = LocalReply::serving(
            vec![(
                Duration::ZERO,
                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec(),
            )],
            Duration::from_secs(3),
        );
        assert_eq!(
            fetch(&server.url, "", Duration::from_millis(500), 1024).unwrap(),
            b"ok"
        );
        assert!(!server.closed.load(std::sync::atomic::Ordering::SeqCst));
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
            Duration::ZERO,
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
}
