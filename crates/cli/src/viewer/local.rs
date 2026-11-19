//! Discovery and bounded HTTP access to the local menu bar viewer.
use super::{MENUBAR_RECORD, SERVICE, unanswered};
use atlas_core::error::{Code, Error, Result};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// The most bytes a reply may take, unless its caller says.
const REPLY_LIMIT: usize = 1 << 16;

/// The JSON record at `path`, when it reads as one.
pub(crate) fn read_record(path: &Path) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Where the record of the menu bar's service that keeps the records in
/// `output` is: its station-wide one while that names `output` among the
/// records folders of the desktops it serves, so every desktop finds the
/// one service the same way; else the one in `output` itself, as a service
/// of an earlier build keeps it.
pub(crate) fn record_for(output: &Path) -> PathBuf {
    match atlas_core::config::home() {
        Ok(home) => record_in(&home, output),
        Err(_) => output.join(MENUBAR_RECORD),
    }
}

/// [`record_for`], the station's folder in `home`.
pub(super) fn record_in(home: &Path, output: &Path) -> PathBuf {
    let own = output.join(MENUBAR_RECORD);
    let station = super::station::service_record(home);
    let names = read_record(&station).is_some_and(|record| {
        record["records"].as_array().is_some_and(|folders| {
            folders
                .iter()
                .any(|folder| folder.as_str().map(Path::new) == Some(output))
        })
    });
    if names { station } else { own }
}

/// The private address `record` names.
pub(crate) fn address(record: &Value) -> Option<&str> {
    record["url"].as_str()
}

/// Whether `record` names a viewer on `port`.
fn serves(record: &Value, port: u16) -> bool {
    address(record).is_some_and(|url| url.starts_with(&format!("http://127.0.0.1:{port}/")))
}

/// The port `record` says its viewer serves.
pub(crate) fn recorded_port(record: &Value) -> Option<u16> {
    address(record)?
        .strip_prefix("http://127.0.0.1:")?
        .split('/')
        .next()?
        .parse()
        .ok()
}

/// The process `reply` names, as a viewer or its record says it.
pub(crate) fn process(reply: &Value) -> Option<u32> {
    reply["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
}

/// The viewer recorded at `record`, if it serves `port`, answers there now
/// as the very process the record names, and serves this version. It blocks
/// for up to a second: call it off the async runtime.
pub(crate) fn running(record: &Path, port: u16) -> Option<Value> {
    identified(record, port)
        .filter(|(_, identity)| compatible(identity))
        .map(|(record, _)| record)
}

/// The viewer recorded at `record`, if it serves `port` and answers there
/// now as the very process the record names, whatever its version, with
/// what it said it is: one reply for both.
fn identified(record: &Path, port: u16) -> Option<(Value, Value)> {
    let running = read_record(record).filter(|running| serves(running, port))?;
    let recorded = process(&running)?;
    let identity = address(&running).and_then(identity)?;
    (process(&identity) == Some(recorded)).then_some((running, identity))
}

/// Whether a viewer that says `identity` serves this version.
fn compatible(identity: &Value) -> bool {
    identity["protocol"].as_u64() == Some(super::PROTOCOL)
}

/// The process serving the viewer that answers at `url` (its private
/// address) right now; `None` when no viewer answers there.
pub(crate) fn answering(url: &str) -> Option<u32> {
    identity(url).and_then(|reply| process(&reply))
}

/// What the viewer that answers at `url` says it is, when one does.
fn identity(url: &str) -> Option<Value> {
    exchange(url, "GET", "", None, Duration::from_secs(1), REPLY_LIMIT)
        .ok()
        .filter(|reply| reply.status == 200)
        .and_then(|reply| serde_json::from_slice::<Value>(&reply.body).ok())
        .filter(|reply| reply["service"] == SERVICE)
}

/// Whether the menu bar's viewer is recorded in `output` as serving `port`
/// and its process still runs Atlas, answering or not.
pub(crate) fn recorded(output: &Path, port: u16) -> bool {
    read_record(&record_for(output))
        .filter(|record| serves(record, port))
        .and_then(|record| process(&record))
        .and_then(|pid| i32::try_from(pid).ok())
        .is_some_and(crate::menubar::is_atlas)
}

/// The private address of the menu bar's viewer, when it runs and answers
/// as the process its record names. It blocks for up to a second: call it
/// off the async runtime.
pub(crate) fn local(output: &Path, port: u16) -> Option<String> {
    local_checked(output, port).ok().flatten()
}

/// [`local`], refusing a menu bar's viewer of another version, which this
/// command cannot talk to: one from before an update, still running.
///
/// # Errors
///
/// Returns a menu bar error saying to restart Atlas.
pub(crate) fn local_checked(output: &Path, port: u16) -> Result<Option<String>> {
    local_serving(output, port).map(|found| found.map(|(url, _)| url))
}

/// [`local_checked`], with the record key of the desktop the viewer says
/// it serves, when it says.
///
/// # Errors
///
/// As [`local_checked`].
pub(crate) fn local_serving(output: &Path, port: u16) -> Result<Option<(String, Option<String>)>> {
    match identified(&record_for(output), port) {
        Some((running, identity)) if compatible(&identity) => Ok(address(&running).map(|url| {
            let served = identity["desktopKey"].as_str().map(str::to_owned);
            (url.to_owned(), served)
        })),
        Some(_) => Err(Error::new(
            Code::Menubar,
            "Atlas in the menu bar is from another version. Restart Atlas: atlas menubar \
             --stop, then atlas menubar.",
        )),
        None => Ok(None),
    }
}

/// The private address of the menu bar's viewer, when it runs, serves
/// this version and keeps a feed of what changes. It blocks for up to a
/// second: call it off the async runtime.
pub(crate) fn serving_events(output: &Path, port: u16) -> Option<String> {
    let (running, identity) = identified(&record_for(output), port)?;
    let events = identity["capabilities"]
        .as_array()
        .is_some_and(|capabilities| capabilities.iter().any(|capability| capability == "events"));
    (compatible(&identity) && events)
        .then(|| address(&running).map(str::to_owned))
        .flatten()
}

/// What the local viewer at `url` serves at `route`, read within `deadline`
/// and at most `limit` bytes.
pub(crate) fn fetch(url: &str, route: &str, deadline: Duration, limit: usize) -> Result<Vec<u8>> {
    Ok(served(url, route, deadline, limit)?.body)
}

/// The description of the frame service the local viewer at `url` runs,
/// when it runs one for `owner`'s pictures.
pub(crate) fn frame_service(
    url: &str,
    owner: &crate::frame_pipe::owner::Owner,
) -> Option<serde_json::Value> {
    let body = fetch(url, "pipe.json", Duration::from_secs(2), 16 * 1024).ok()?;
    let descriptor: serde_json::Value = serde_json::from_slice(&body).ok()?;
    owner.serves(&descriptor).then_some(descriptor)
}

/// The menu bar recorded at `record`, while it runs: its address and the
/// description of the frame service it runs for `owner`'s pictures. Every
/// observer of a desktop, the app and the terminal alike, finds that one
/// service so. It blocks for a few seconds at most: call it off the async
/// runtime.
pub(crate) fn shared_service(
    record: &Path,
    owner: &crate::frame_pipe::owner::Owner,
) -> Option<(String, serde_json::Value)> {
    let found = read_record(record)?;
    let port = recorded_port(&found)?;
    let running = running(record, port)?;
    let url = address(&running)?.to_owned();
    frame_service(&url, owner).map(|descriptor| (url, descriptor))
}

/// A GET from the local viewer at `url`. Anything but a 200 is refused.
fn served(url: &str, route: &str, deadline: Duration, limit: usize) -> Result<Reply> {
    let reply = exchange(url, "GET", route, None, deadline, limit)?;
    (reply.status == 200)
        .then_some(reply)
        .ok_or_else(unanswered)
}

/// The menu bar viewer's newest frame, with the key control requests name
/// it by and the display's size in points, which input is given in.
pub(crate) struct SharedFrame {
    pub(crate) png: Vec<u8>,
    /// A live frame's packed RGB pixels, in place of a PNG.
    pub(crate) rgb: Option<std::sync::Arc<atlas_core::buffer::Buffer>>,
    /// A live frame's size in pixels.
    pub(crate) pixels: (u32, u32),
    pub(crate) metrics: Option<atlas_client::client::live::Metrics>,
    pub(crate) key: String,
    pub(crate) display: (f64, f64),
}

/// The local viewer's newest frame at `url`, of the desktop `desktop`
/// names by its ID when it names one, read within `deadline` and at most
/// `limit` bytes. One the viewer gives only the person's app, a VM's, or
/// one of a desktop it does not serve, is refused in its own words.
pub(crate) fn fetch_frame(
    url: &str,
    desktop: Option<&str>,
    deadline: Duration,
    limit: usize,
) -> Result<SharedFrame> {
    let route = desktop.map_or_else(
        || "frame.png".to_owned(),
        |desktop| format!("frame.png?desktop={desktop}"),
    );
    let reply = exchange(url, "GET", &route, None, deadline, limit)?;
    if matches!(reply.status, 403 | 409)
        && let Ok(refused) = serde_json::from_slice::<Value>(&reply.body)
    {
        return Err(Error::refused(
            Code::of_reply(&refused, Code::Menubar),
            &refused,
            "The menu bar's viewer refused",
        ));
    }
    if reply.status != 200 {
        return Err(unanswered());
    }
    let named = |name: &str| reply_header(&reply.wire, name);
    let points = |name: &str| named(name).and_then(|value| value.parse::<f64>().ok());

    Ok(SharedFrame {
        png: reply.body,
        rgb: None,
        pixels: (0, 0),
        metrics: None,
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

/// A completed reply, with its wire bytes for header lookup.
pub(super) struct Reply {
    wire: Vec<u8>,
    pub(super) status: u16,
    body: Vec<u8>,
}

/// One request to the local viewer at `url`, as its own origin, within
/// `deadline` overall; a reply larger than `limit` bytes is refused.
pub(super) fn exchange(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    deadline: Duration,
    limit: usize,
) -> Result<Reply> {
    exchange_with(url, method, route, body, &[], deadline, limit)
}

/// [`exchange`], sending `headers` too.
fn exchange_with(
    url: &str,
    method: &str,
    route: &str,
    body: Option<&[u8]>,
    headers: &[(&str, &str)],
    deadline: Duration,
    limit: usize,
) -> Result<Reply> {
    use std::io::{Read, Write};

    // Any trouble on the wire reads the same to the person.
    let io = |error: std::io::Error| unanswered().with_source(error);

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
    stream.set_nodelay(true).map_err(io)?;
    stream.set_write_timeout(Some(left()?)).map_err(io)?;
    let mut head =
        format!("{method} {path}{route} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n");

    if let Some(body) = body {
        head.push_str(&format!(
            "Origin: http://{authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }

    head.push_str("\r\n");
    // One write: a separate small body can wait behind Nagle and delayed
    // ACKs even on the loopback interface.
    let mut request = head.into_bytes();
    if let Some(body) = body {
        request.extend_from_slice(body);
    }
    stream.write_all(&request).map_err(io)?;

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
fn parse_response(reply: &[u8]) -> Option<(u16, Vec<u8>)> {
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
        return Some((status, unchunked(rest)?));
    }

    Some((status, rest.get(..length?)?.to_vec()))
}

/// A chunked body whole; None until its last chunk has arrived.
fn unchunked(mut at: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();

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
            return Some(body);
        }

        body.extend_from_slice(at.get(..size)?);
        at = at.get(size + 2..)?;
    }
}

/// A complete reply's status, for tests that read a raw connection.
#[cfg(test)]
pub(super) fn reply_status(reply: &[u8]) -> Option<u16> {
    parse_response(reply).map(|(status, _)| status)
}

/// POST `body` to `route` of the local viewer at `url`, as its own origin,
/// and read its JSON reply; a refusal is an error in its own words.
pub(crate) fn post_local(url: &str, route: &str, body: &Value) -> Result<Value> {
    post_local_with(url, route, body, &[])
}

/// [`post_local`] as the app, which names itself by `secret`.
#[cfg(test)]
pub(crate) fn post_local_as_app(
    url: &str,
    secret: &str,
    route: &str,
    body: &Value,
) -> Result<Value> {
    post_local_with(
        url,
        route,
        body,
        &[(super::http::APP_SECRET_HEADER, secret)],
    )
}

/// [`post_local`], sending `headers` too.
fn post_local_with(
    url: &str,
    route: &str,
    body: &Value,
    headers: &[(&str, &str)],
) -> Result<Value> {
    let exchanged = exchange_with(
        url,
        "POST",
        route,
        Some(&serde_json::to_vec(body)?),
        headers,
        Duration::from_secs(5),
        REPLY_LIMIT,
    )?;
    let reply: Value = serde_json::from_slice(&exchanged.body).map_err(|_| unanswered())?;

    if exchanged.status != 200 {
        // A refusal keeps its own code (busy, stale-frame…) when it is one
        // Atlas knows.
        return Err(Error::refused(
            Code::of_reply(&reply, Code::Menubar),
            &reply,
            "The menu bar's viewer refused",
        ));
    }

    Ok(reply)
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
