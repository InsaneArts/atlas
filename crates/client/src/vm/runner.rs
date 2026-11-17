//! The station's client of a VM's runner, Atlas VM, on its control socket:
//! each of its operations as a typed request and its reply as a typed
//! answer. A read is asked as it is; a change is asked only once its ID and
//! request are kept in the VM's lifecycle record, so a change cut short is
//! known, and asked again as the very same request.
use super::{System, Vms, lifecycle::record::Record};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// What Atlas VM's guest install is given: the Atlas.app to install, the
/// enrolment its guest pairs for, and the pairing secret, which is sent and
/// never kept.
#[derive(Clone, PartialEq, Eq)]
pub struct Install {
    pub bundle: PathBuf,
    pub enrolment: Token,
    secret: String,
}

impl Install {
    /// `bundle`, for `enrolment`, with `secret`.
    #[must_use]
    pub fn new(bundle: PathBuf, enrolment: Token, secret: String) -> Self {
        Self {
            bundle,
            enrolment,
            secret,
        }
    }

    /// The secret it hands over, for a test that stands in for Atlas VM.
    #[cfg(test)]
    pub(crate) fn secret(&self) -> &str {
        &self.secret
    }
}

impl std::fmt::Debug for Install {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Install")
            .field("bundle", &self.bundle)
            .field("enrolment", &self.enrolment)
            .finish_non_exhaustive()
    }
}

/// What the station asks a VM's runner: a read, or a change with its ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Its version and what it can do.
    Hello,
    /// The VM's power, as it sees it.
    State,
    /// What the change with this ID came to.
    Outcome(Token),
    /// Make the VM.
    Create(Token),
    /// Start it.
    Start(Token),
    /// Stop it.
    Stop(Token),
    /// Open its window with input, or close it.
    Window { change: Token, input: bool },
    /// Put Atlas inside its guest.
    Install { change: Token, install: Install },
    /// Go on with the install of this change, already handed its secret,
    /// without it: its close, the guest account no administrator and
    /// Remote Login off again. It is no change of its own: the install's
    /// change answers it.
    InstallResume(Token),
    /// The station's word that the guest of the install `install` paired
    /// for `enrolment`.
    Paired {
        change: Token,
        install: Token,
        enrolment: Token,
    },
    /// The station's word that the guest of the install `install` says,
    /// over its authenticated session, that Remote Login is off and its
    /// account no administrator.
    Closed { change: Token, install: Token },
    /// The VM removed: stopped, its passwords deleted, its runner gone.
    Forget(Token),
}

impl Operation {
    /// Its name on the socket.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::State => "state",
            Self::Outcome(_) => "outcome",
            Self::Create(_) => "create",
            Self::Start(_) => "start",
            Self::Stop(_) => "stop",
            Self::Window { .. } => "window",
            Self::Install { .. } => "install",
            Self::InstallResume(_) => "install-resume",
            Self::Paired { .. } => "paired",
            Self::Closed { .. } => "closed",
            Self::Forget(_) => "forget",
        }
    }

    /// The change it is; none for a read.
    #[must_use]
    pub fn change(&self) -> Option<&Token> {
        match self {
            Self::Hello | Self::State | Self::Outcome(_) => None,
            Self::Create(change)
            | Self::Start(change)
            | Self::Stop(change)
            | Self::Forget(change)
            | Self::InstallResume(change)
            | Self::Window { change, .. }
            | Self::Install { change, .. }
            | Self::Paired { change, .. }
            | Self::Closed { change, .. } => Some(change),
        }
    }

    /// The enrolment it names, when it names one.
    #[must_use]
    pub fn enrolment(&self) -> Option<&Token> {
        match self {
            Self::Install { install, .. } => Some(&install.enrolment),
            Self::Paired { enrolment, .. } => Some(enrolment),
            _ => None,
        }
    }

    /// The request line it is sent as, a secret included.
    fn request(&self) -> Value {
        self.written(|install| install.secret.clone())
    }

    /// What identifies the request, as a record keeps it: the SHA-256 of
    /// the request with an install's secret replaced by its own digest, as
    /// Atlas VM keeps it, so no record holds the secret.
    #[must_use]
    pub fn digest(&self) -> String {
        let recorded = self.written(|install| format!("sha256:{}", hex(&install.secret)));
        hex(&recorded.to_string())
    }

    /// The request, with an install's secret as `secret` makes it.
    fn written(&self, secret: impl Fn(&Install) -> String) -> Value {
        let op = self.name();
        match self {
            Self::Hello | Self::State => json!({"op": op}),
            Self::Outcome(change)
            | Self::Create(change)
            | Self::Start(change)
            | Self::Stop(change)
            | Self::Forget(change)
            | Self::InstallResume(change) => json!({"op": op, "change": change}),
            Self::Window { change, input } => json!({"op": op, "change": change, "input": input}),
            Self::Install { change, install } => json!({
                "op": op,
                "change": change,
                "bundle": install.bundle.to_string_lossy(),
                "enrolment": install.enrolment,
                "secret": secret(install),
            }),
            Self::Paired {
                change,
                install,
                enrolment,
            } => json!({"op": op, "change": change, "install": install, "enrolment": enrolment}),
            Self::Closed { change, install } => {
                json!({"op": op, "change": change, "install": install})
            }
        }
    }
}

/// The SHA-256 of `text`, as lowercase hexadecimal digits.
fn hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The run of a runner that answered.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incarnation {
    pub pid: i32,
    pub start_seconds: i64,
    pub start_microseconds: i64,
    pub id: String,
}

/// What a runner declares: a version, and what it can do, by name.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Hello {
    pub version: u32,
    pub capabilities: Vec<String>,
}

impl Hello {
    /// Whether it declares `capability`: a station uses only what is
    /// declared.
    #[must_use]
    pub(super) fn can(&self, capability: &str) -> bool {
        self.capabilities
            .iter()
            .any(|declared| declared == capability)
    }
}

/// The VM's power, as its runner sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Power {
    Absent,
    Stopped,
    Starting,
    Running,
    Stopping,
    Unknown,
}

/// Whether the VM's window is open, as its runner sees it: anything it
/// says that this build does not know is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Window {
    Open,
    Closed,
    #[serde(other)]
    Unknown,
}

/// What a change came to.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Outcome {
    Done,
    Refused { reason: String },
}

/// A runner's answer to one request.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Reply {
    /// Whether the request was served.
    pub ok: bool,
    /// The operation answered, when it was read.
    pub op: Option<String>,
    pub incarnation: Option<Incarnation>,
    /// Its declaration, for a hello.
    pub hello: Option<Hello>,
    /// The VM's power, for a state.
    pub power: Option<Power>,
    /// Whether `power` was seen just now.
    pub confirmed: Option<bool>,
    /// Whether its window is open, for a state; none from a runner of a
    /// build before it said so.
    pub window: Option<Window>,
    /// The change answered.
    pub change: Option<Token>,
    /// What the change came to; none while it is pending.
    pub outcome: Option<Outcome>,
    /// Whether the change is kept as asked and not yet settled.
    pub pending: Option<bool>,
    /// Why the request was not served.
    pub error: Option<String>,
}

/// A VM's runner, reached on its control socket.
#[derive(Debug)]
pub struct Runner {
    socket: PathBuf,
}

impl Runner {
    /// The runner of this account's VM `vm`.
    ///
    /// # Errors
    ///
    /// Fails when this account cannot be read.
    pub fn of(vm: &DesktopId) -> Result<Self> {
        Ok(Self {
            socket: socket(&Vms::own()?, vm),
        })
    }

    /// Ask the read `operation`: a hello, the VM's state or a change's
    /// outcome.
    ///
    /// # Errors
    ///
    /// Refuses a change, which is asked only through [`Self::change`];
    /// fails when the runner does not answer whole, or answers what is not
    /// a reply.
    pub fn read(&self, operation: &Operation) -> Result<Reply> {
        read(&super::Real, &self.socket, operation)
    }

    /// [`Self::read`], the whole exchange on the socket within `within`, so
    /// nothing waits on a runner that does not answer past it.
    ///
    /// # Errors
    ///
    /// As [`Self::read`], and when `within` passes first.
    pub(super) fn read_within(&self, operation: &Operation, within: Duration) -> Result<Reply> {
        refuse_change(operation)?;
        reply(super::control::ask_within(
            &self.socket,
            &operation.request(),
            within,
        )?)
    }

    /// The runner listening at `socket`, for a test that stands in for one.
    #[cfg(test)]
    pub(crate) fn at(socket: PathBuf) -> Self {
        Self { socket }
    }

    /// Ask the change `operation`, once its ID and request are kept in the
    /// VM's lifecycle record `record` through `keep`.
    ///
    /// # Errors
    ///
    /// Refuses a read, and a change kept under its ID as another request;
    /// fails when the record cannot be kept, which sends nothing, when the
    /// runner does not answer whole, which says nothing of whether it took
    /// effect, and when it answers for another change.
    pub fn change(
        &self,
        record: &mut Record,
        keep: impl FnOnce(&Record) -> Result<()>,
        operation: &Operation,
    ) -> Result<Reply> {
        change(&super::Real, &self.socket, record, keep, operation)
    }
}

/// The control socket of `vm` in `vms`.
fn socket(vms: &Vms, vm: &DesktopId) -> PathBuf {
    vms.folder(vm).join("control")
}

fn read(system: &impl System, socket: &Path, operation: &Operation) -> Result<Reply> {
    refuse_change(operation)?;
    ask(system, socket, operation)
}

/// Refuse a change asked as a read: it is asked only once it is kept.
fn refuse_change(operation: &Operation) -> Result<()> {
    if operation.change().is_some() {
        return Err(Error::new(
            Code::BadRequest,
            "A change of Atlas VM is asked only once it is kept.",
        ));
    }
    Ok(())
}

fn change(
    system: &impl System,
    socket: &Path,
    record: &mut Record,
    keep: impl FnOnce(&Record) -> Result<()>,
    operation: &Operation,
) -> Result<Reply> {
    record.sending(operation)?;
    keep(record)?;
    let reply = ask(system, socket, operation)?;
    if reply.ok && reply.change.as_ref() != operation.change() {
        return Err(Error::new(
            Code::Transport,
            "The VM's runner answered for another change.",
        ));
    }
    Ok(reply)
}

fn ask(system: &impl System, socket: &Path, operation: &Operation) -> Result<Reply> {
    reply(system.ask(socket, &operation.request())?)
}

/// The runner's `answer` as a reply.
fn reply(answer: Value) -> Result<Reply> {
    serde_json::from_value(answer).map_err(|error| {
        Error::new(
            Code::Transport,
            "The VM's runner answered what is not a reply.",
        )
        .with_source(error)
    })
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
