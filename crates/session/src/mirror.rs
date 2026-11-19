//! Mirrors of the station's loopback endpoints in a desktop: what each end
//! names, the bounds both keep, and the relay both run, which carries one
//! mirrored connection over one lane, each way through bulk credit.
use crate::{control::Outbox, credit::Credit, lane::LaneId};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The most endpoints one list mirrors.
pub const ENDPOINTS: usize = 32;
/// The most connections waiting for their lane at once.
pub const PENDING: usize = 12;
/// The most mirror lanes at once, opening and joined together, on each end.
pub const LANES: usize = 12;
/// The most read at once on either side of a relay, and so the most of a
/// mirror lane's bytes on their way at once.
pub const CHUNK: usize = 64 * 1024;

/// An address family on loopback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Family {
    /// `127.0.0.1`.
    #[serde(rename = "v4")]
    Ipv4,
    /// `[::1]`, on IPv6 alone.
    #[serde(rename = "v6")]
    Ipv6,
}

/// One loopback endpoint the station listens on, as listed: `incarnation`
/// grows each time it is listed again after leaving the list, and every
/// connection is bound to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub family: Family,
    pub port: u16,
    pub incarnation: u64,
}

impl Endpoint {
    /// Its family and port, the same whatever its incarnation.
    pub fn place(&self) -> (Family, u16) {
        (self.family, self.port)
    }

    /// Its address on loopback.
    pub fn address(&self) -> std::net::SocketAddr {
        match self.family {
            Family::Ipv4 => (std::net::Ipv4Addr::LOCALHOST, self.port).into(),
            Family::Ipv6 => (std::net::Ipv6Addr::LOCALHOST, self.port).into(),
        }
    }
}

/// Why an endpoint could not be listened on in the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// Something in the desktop holds it already.
    InUse,
    /// The desktop may not listen there.
    Denied,
    Failed,
}

/// An endpoint the desktop could not listen on, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refused {
    pub endpoint: Endpoint,
    pub reason: Reason,
}

/// Whether `endpoints` is a list a desktop may take: within its bound, no
/// port 0, and no place twice.
pub fn takeable(endpoints: &[Endpoint]) -> bool {
    let mut places = std::collections::HashSet::new();
    endpoints.len() <= ENDPOINTS
        && endpoints
            .iter()
            .all(|endpoint| endpoint.port != 0 && places.insert(endpoint.place()))
}

/// Gives a lane's sending credit back however its relay ends, dropped
/// included: declared before the pumps, so it goes after them.
struct Closing<'a> {
    credit: &'a Credit,
    lane: LaneId,
}

impl Drop for Closing<'_> {
    fn drop(&mut self) {
        self.credit.closed(self.lane);
    }
}

/// Carry one mirrored connection, `local`, over the lane `lane`, read from
/// `from_lane` and written to `to_lane`. What goes onto the lane spends
/// `sending`, a chunk at a time, each waiting until the last is repaid;
/// what comes off it is credited back on `returning`. The first end of
/// stream, from either side, is passed on as a half-close only. It ends
/// once both sides ended, either fails, or the other end gives the lane up.
pub async fn carry<T, R, W>(
    local: T,
    mut from_lane: R,
    mut to_lane: W,
    lane: LaneId,
    sending: &Credit,
    returning: &Outbox,
) where
    T: AsyncRead + AsyncWrite + Unpin,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let _closing = Closing {
        credit: sending,
        lane,
    };
    let (mut from_local, mut to_local) = tokio::io::split(local);
    let outward = async {
        let mut buffer = vec![0_u8; CHUNK];
        loop {
            let read = from_local.read(&mut buffer).await.ok()?;
            if read == 0 {
                to_lane.shutdown().await.ok()?;
                return Some(());
            }
            sending
                .send(lane, &mut to_lane, &buffer[..read])
                .await
                .ok()?;
            to_lane.flush().await.ok()?;
            sending.repaid(lane).await;
        }
    };
    let inward = async {
        let mut buffer = vec![0_u8; CHUNK];
        loop {
            let read = from_lane.read(&mut buffer).await.ok()?;
            if read == 0 {
                to_local.shutdown().await.ok()?;
                return Some(());
            }
            to_local.write_all(&buffer[..read]).await.ok()?;
            returning
                .say(crate::control::Said::Credit {
                    lane,
                    bytes: read as u64,
                })
                .ok()?;
        }
    };
    tokio::pin!(outward, inward);
    // Both run until each has ended. A failure of either ends both, and the
    // other end is told the lane is given up, so a pump of its own waiting
    // to be repaid, its other way ended, is never left waiting for good; the
    // other end giving the lane up ends this one alike.
    let given_up = || {
        let _ = returning.say(crate::control::Said::Credit {
            lane,
            bytes: crate::credit::ENDED,
        });
    };
    let (mut out_done, mut in_done) = (false, false);
    while !(out_done && in_done) {
        tokio::select! {
            ended = &mut outward, if !out_done => {
                if ended.is_none() {
                    given_up();
                    return;
                }
                out_done = true;
            }
            ended = &mut inward, if !in_done => {
                if ended.is_none() {
                    given_up();
                    return;
                }
                in_done = true;
            }
            () = sending.given_up(lane) => return,
        }
    }
}

/// The longest the station takes to open a mirror lane, or to connect to
/// its own endpoint for one.
pub const SETTING_UP: std::time::Duration = std::time::Duration::from_secs(10);

/// Cancels a mirror lane on the desktop however its relay ends, dropped
/// included, so the desktop's end of the connection never outlives the
/// station's.
struct Cancelling<'a> {
    desktop: &'a crate::station::Desktop,
    lane: LaneId,
}

impl Drop for Cancelling<'_> {
    fn drop(&mut self) {
        self.desktop.cancel(self.lane);
    }
}

/// The station's side of one mirrored connection: open the lane for the
/// guest's `connection` accepted on `endpoint`, connect to `target` on
/// this Mac, which is the endpoint itself in production, and carry the two
/// until both end, once `serves`, given the connection's own address and
/// its peer's, passes whoever took it. Opening and connecting each have
/// [`SETTING_UP`]. However it ends, the lane is cancelled.
///
/// # Errors
///
/// Fails when the lane cannot be opened or the endpoint reached in time,
/// and as `serves` refuses, nothing carried either way.
pub async fn relay<Checked>(
    desktop: &crate::station::Desktop,
    sending: &Credit,
    returning: &Outbox,
    connection: u64,
    endpoint: Endpoint,
    target: std::net::SocketAddr,
    serves: impl FnOnce(std::net::SocketAddr, std::net::SocketAddr) -> Checked,
) -> atlas_core::error::Result<()>
where
    Checked: std::future::Future<Output = atlas_core::error::Result<()>>,
{
    use atlas_core::error::{Code, Error};
    let late = |what: &str| {
        Error::new(
            Code::Transport,
            format!("The mirror's {what} took too long."),
        )
    };
    let kind = crate::lane::LaneKind::Mirror {
        connection,
        endpoint,
    };
    let opened = tokio::time::timeout(SETTING_UP, desktop.open(kind))
        .await
        .map_err(|_| late("lane"))??;
    let _cancelling = Cancelling {
        desktop,
        lane: opened.lane,
    };
    let unreached = |error: std::io::Error| {
        Error::new(
            Code::Transport,
            "The mirrored endpoint could not be reached.",
        )
        .with_source(error)
    };
    let local = tokio::time::timeout(SETTING_UP, tokio::net::TcpStream::connect(target))
        .await
        .map_err(|_| late("connection"))?
        .map_err(unreached)?;
    let own = local.local_addr().map_err(unreached)?;
    let peer = local.peer_addr().map_err(unreached)?;
    serves(own, peer).await?;
    let (from_lane, to_lane) = tokio::io::split(opened.stream);
    carry(local, from_lane, to_lane, opened.lane, sending, returning).await;
    Ok(())
}

#[cfg(test)]
#[path = "mirror_tests.rs"]
mod tests;
