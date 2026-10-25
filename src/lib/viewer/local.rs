//! Discovery and bounded HTTP access to the local menu bar viewer.
use super::{MENUBAR_RECORD, SERVICE, unanswered};
use crate::error::{Error, Result};
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
    parse_response(&reply).ok_or_else(unanswered)
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
    let Some((200, png)) = parse_response(&reply) else {
        return Err(unanswered());
    };
    let named = |name: &str| reply_header(&reply, name);
    let points = |name: &str| named(name).and_then(|value| value.parse::<f64>().ok());

    Ok(SharedFrame {
        png,
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

/// One request to the local viewer at `url`: its whole reply, head and
/// body, once all of it has arrived.
fn exchange_raw(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    deadline: Duration,
    limit: usize,
) -> Result<Vec<u8>> {
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

        if parse_response(&reply).is_some() {
            return Ok(reply);
        }
    }

    Ok(reply)
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
            .and_then(crate::error::code)
            .unwrap_or("menubar");
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
}
