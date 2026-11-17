//! How the station reaches a desktop's command service: the helper login's
//! Unix socket, whose peer the kernel names, or a lane of a machine
//! desktop's session, whose peer its handshake proves; a desktop this build
//! cannot reach yet has neither, and is refused.
use atlas_core::{
    config::desktops::Selected,
    error::{Code, Error, Result},
    files::Token,
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

mod keeper;

pub use keeper::Keeper;
pub(crate) use keeper::Launchd;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use keeper::Trust;
pub(crate) use keeper::probe;

#[derive(Clone, Debug)]
pub enum Link {
    Unix(Transport),
    Session(Session),
    /// A VM desktop's session keeper: its own calls, the reads it carries
    /// to the guest and the changes a command makes; every other request is
    /// refused.
    Keeper(Keeper),
    Unreachable(Unreachable),
}

/// A machine desktop's live generation, as the station reaches it.
#[derive(Clone, Debug)]
pub struct Session {
    pub desktop: Arc<Desktop>,
    pub timeout: Duration,
}

/// A desktop this build cannot reach yet: every request is refused, in the
/// one wording its settings give, before anything is opened.
#[derive(Clone, Debug)]
pub struct Unreachable {
    pub selected: Arc<Selected>,
    pub timeout: Duration,
    /// Why it cannot be reached now, when that is known: its keeper is not
    /// running, say, or it is not paired yet.
    pub why: Option<String>,
}

impl Link {
    /// How long a request has to be answered.
    pub fn timeout(&self) -> Duration {
        match self {
            Self::Unix(transport) => transport.timeout,
            Self::Session(session) => session.timeout,
            Self::Keeper(keeper) => keeper.timeout,
            Self::Unreachable(unreachable) => unreachable.timeout,
        }
    }

    /// The same link, its requests answered within `timeout`.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        match &mut self {
            Self::Unix(transport) => transport.timeout = timeout,
            Self::Session(session) => session.timeout = timeout,
            Self::Keeper(keeper) => keeper.timeout = timeout,
            Self::Unreachable(unreachable) => unreachable.timeout = timeout,
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
            Self::Keeper(keeper) => Err(keeper.selected.unreachable()),
            Self::Unreachable(unreachable) => Err(unreachable.selected.unreachable()),
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
            Self::Session(_) | Self::Keeper(_) | Self::Unreachable(_) => {
                panic!("a machine desktop has no socket")
            }
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
            Self::Keeper(keeper) => Err(keeper.selected.unreachable()),
            Self::Unreachable(unreachable) => Err(unreachable.selected.unreachable()),
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
            // Its keeper carries its cancel, and only the reads it serves.
            Self::Keeper(keeper) => {
                return Ok((keeper.send_read(request).await?, Unanswered(None)));
            }
            Self::Unreachable(unreachable) => return Err(unreachable.selected.unreachable()),
        };
        let (pending, unanswered) = self.connect(kind).await?;
        Ok((pending.dispatch(request).await?, unanswered))
    }

    /// A connection to a VM desktop's keeper for `request`, the change
    /// `route` names, in the generation `generation` its command proved,
    /// made and trusted within this link's deadline, and what is to be said
    /// on it: nothing is sent yet. A keeper cancels a change whose client
    /// goes before its final reply, so nothing is left to cancel here.
    ///
    /// # Errors
    ///
    /// Refuses every other link, and a read on an input lane the keeper
    /// does not carry in the one wording; fails when the keeper cannot be
    /// reached in time or trusted. Nothing was sent whichever.
    pub(crate) async fn connect_change(
        &self,
        route: &Route,
        request: &Value,
        generation: u64,
    ) -> Result<(Pending<Stream>, Value)> {
        match self {
            Self::Keeper(keeper) => {
                let asked = keeper.change(route, request, generation)?;
                Ok((keeper.connected().await?, asked))
            }
            Self::Unix(_) | Self::Session(_) => Err(Error::new(
                Code::Platform,
                "Only a VM desktop takes its changes through its keeper.",
            )),
            Self::Unreachable(unreachable) => Err(unreachable.selected.unreachable()),
        }
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

/// Which lane of a VM desktop's session a request goes on: a change of the
/// guest's daemon, a stage of a file or folder into it, or its input
/// service's own lane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    /// A change the guest's daemon carries out, named by its token.
    Change(Token),
    /// A stage, named by its token, whose bytes follow its request once
    /// the guest says it is ready for them.
    Stage(Token),
    /// The guest's input service: a change to what it holds or does, named
    /// by its token, or a read, which names none.
    Input(Option<Token>),
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
    /// A VM's session keeper, trusted on this very connection.
    Keeper(UnixStream),
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Unix(stream) | Self::Keeper(stream) => {
                Pin::new(stream).poll_read(context, buffer)
            }
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
            Self::Unix(stream) | Self::Keeper(stream) => {
                Pin::new(stream).poll_write(context, bytes)
            }
            Self::Lane(lane) => Pin::new(lane.as_mut()).poll_write(context, bytes),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Unix(stream) | Self::Keeper(stream) => Pin::new(stream).poll_flush(context),
            Self::Lane(lane) => Pin::new(lane.as_mut()).poll_flush(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Unix(stream) | Self::Keeper(stream) => Pin::new(stream).poll_shutdown(context),
            Self::Lane(lane) => Pin::new(lane.as_mut()).poll_shutdown(context),
        }
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
