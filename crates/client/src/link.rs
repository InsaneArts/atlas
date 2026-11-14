//! How the station reaches a desktop's command service: the helper login's
//! Unix socket, whose peer the kernel names, or a lane of a machine
//! desktop's session, whose peer its handshake proves.
use atlas_core::{
    error::{Code, Error, Result},
    protocol::{self, ConnectionPhase},
    transport::{Pending, Transport, refusal as refusal_of},
};
use atlas_session::{
    lane::{LaneId, LaneKind},
    secure::Secure,
    station::{Desktop, Raw},
};
use serde_json::Value;
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::UnixStream,
    time::{Instant, timeout_at},
};

#[derive(Clone, Debug)]
pub enum Link {
    Unix(Transport),
    Session(Session),
}

/// A machine desktop's live generation, as the station reaches it.
#[derive(Clone, Debug)]
pub struct Session {
    pub desktop: Arc<Desktop>,
    pub timeout: Duration,
}

impl Link {
    /// How long a request has to be answered.
    pub fn timeout(&self) -> Duration {
        match self {
            Self::Unix(transport) => transport.timeout,
            Self::Session(session) => session.timeout,
        }
    }

    /// The same link, its requests answered within `timeout`.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        match &mut self {
            Self::Unix(transport) => transport.timeout = timeout,
            Self::Session(session) => session.timeout = timeout,
        }
        self
    }

    /// The helper login's socket, for what only it does.
    ///
    /// # Errors
    ///
    /// Refuses a machine desktop, which does not do it yet.
    pub fn socket(&self) -> Result<&Transport> {
        match self {
            Self::Unix(transport) => Ok(transport),
            Self::Session(session) => Err(Error::new(
                Code::Platform,
                format!(
                    "Desktop {} cannot do this yet.",
                    session.desktop.id().as_str()
                ),
            )),
        }
    }

    /// The helper login's socket, for a test that moves it.
    ///
    /// # Panics
    ///
    /// On a machine desktop.
    #[cfg(any(test, feature = "test-support"))]
    pub fn socket_mut(&mut self) -> &mut Transport {
        match self {
            Self::Unix(transport) => transport,
            Self::Session(_) => panic!("a machine desktop has no socket"),
        }
    }

    /// A connection for a request of `kind`, its peer proved, and what
    /// cancels the request's work should it go unanswered.
    ///
    /// # Errors
    ///
    /// Returns the failure to connect or to prove the peer.
    async fn connect(&self, kind: LaneKind) -> Result<(Pending<Stream>, Unanswered)> {
        match self {
            Self::Unix(transport) => Ok((
                transport.connect().await?.map(Stream::Unix),
                Unanswered(None),
            )),
            Self::Session(session) => {
                // One deadline for the whole request, its dial and handshake
                // included.
                let deadline = Instant::now() + session.timeout;
                // Boxed: a handshake's future is large, and every request's
                // future would carry it inline even on the helper login's socket.
                let opened = timeout_at(deadline, Box::pin(session.desktop.open(kind)))
                    .await
                    .map_err(|elapsed| {
                        Error::new(
                            Code::Transport,
                            format!(
                                "Timed out reaching desktop {}",
                                session.desktop.id().as_str()
                            ),
                        )
                        .with_source(elapsed)
                    })??;
                let unanswered = Unanswered(Some((session.desktop.clone(), opened.lane)));
                let pending = Pending::over_authenticated(
                    Stream::Lane(Box::new(opened.stream)),
                    deadline.saturating_duration_since(Instant::now()),
                );
                Ok((pending, unanswered))
            }
        }
    }

    /// Send `request` on the lane its operation needs, judged from this
    /// very request: what comes back is its reply to read, and what cancels
    /// it should that reply never come.
    ///
    /// # Errors
    ///
    /// Returns the failure to connect, to prove the peer or to send, and
    /// refuses a change to a machine desktop before anything is opened: it
    /// does not keep the records changes need yet.
    pub async fn send(&self, request: &Value) -> Result<(Pending<Stream>, Unanswered)> {
        let kind = match self {
            Self::Unix(_) => LaneKind::Read,
            Self::Session(session) => lane_for(session, request)?,
        };
        let (pending, unanswered) = self.connect(kind).await?;
        Ok((pending.dispatch(request).await?, unanswered))
    }

    /// Exchange one request with the desktop within this link's deadline.
    ///
    /// # Errors
    ///
    /// Returns connection, framing, timeout or reply failures. A failure
    /// after dispatch can be uncertain, and is never safe to replay.
    pub async fn call(&self, request: &Value) -> Result<Value> {
        let (sent, unanswered) = self.send(request).await?;
        let reply = sent.finish().await?;
        unanswered.answered();
        Ok(reply)
    }
}

/// The lane `request` goes on to `session`'s desktop, judged by its
/// operation as the desktop judges it: a read on a lane that reads.
fn lane_for(session: &Session, request: &Value) -> Result<LaneKind> {
    let line = serde_json::to_vec(request)?;
    let asked = protocol::parse(&line).map_err(|refusal| refusal_of(&refusal))?;
    match asked.request.policy().phase {
        ConnectionPhase::Read(_) => Ok(LaneKind::Read),
        ConnectionPhase::Mutation(operation) => Err(Error::new(
            Code::Platform,
            format!(
                "Desktop {} cannot do {operation} yet.",
                session.desktop.id().as_str()
            ),
        )),
        ConnectionPhase::Refusal => Err(Error::new(
            Code::BadRequest,
            "The request cannot be sent to a desktop.",
        )),
    }
}

/// A request whose final reply has not come. Dropped so, its lane's work
/// is cancelled over control: the lane's connection closing says nothing
/// to the desktop's work. The helper login's socket needs none, as its
/// daemon ends a request whose connection goes.
#[derive(Debug)]
#[must_use = "dropping it cancels the request"]
pub struct Unanswered(Option<(Arc<Desktop>, LaneId)>);

impl Unanswered {
    /// The final reply came: nothing to cancel.
    pub fn answered(mut self) {
        self.0 = None;
    }
}

impl Drop for Unanswered {
    fn drop(&mut self) {
        if let Some((desktop, lane)) = self.0.take() {
            desktop.cancel(lane);
        }
    }
}

/// A connection to a desktop's command service.
#[derive(Debug)]
pub enum Stream {
    Unix(UnixStream),
    Lane(Box<Secure<Raw>>),
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Unix(stream) => Pin::new(stream).poll_read(context, buffer),
            Self::Lane(lane) => Pin::new(lane.as_mut()).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Unix(stream) => Pin::new(stream).poll_write(context, bytes),
            Self::Lane(lane) => Pin::new(lane.as_mut()).poll_write(context, bytes),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Unix(stream) => Pin::new(stream).poll_flush(context),
            Self::Lane(lane) => Pin::new(lane.as_mut()).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Unix(stream) => Pin::new(stream).poll_shutdown(context),
            Self::Lane(lane) => Pin::new(lane.as_mut()).poll_shutdown(context),
        }
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
