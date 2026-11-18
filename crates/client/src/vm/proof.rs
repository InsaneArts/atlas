//! What a VM's proof is judged on: its keeper's observation of one
//! generation, read whole and held to the challenge this command made, and
//! what the station holds the guest to beside its pairing: that Atlas VM
//! runs it, in a virtual machine, with its window closed to the person's
//! input.
use super::runner::{Power, Window};
use atlas_core::{
    desktop::Desktop,
    error::{Code, Error, Result},
    session::{MachineExpect, MachineSession, Observation, key::PeerKey, paired::Paired},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What Atlas VM said of the VM while it was observed.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Runner {
    #[serde(default)]
    pub(crate) power: Option<Power>,
    #[serde(default)]
    pub(crate) confirmed: Option<bool>,
    #[serde(default)]
    pub(crate) window: Option<Window>,
    /// The run of Atlas VM that said it.
    #[serde(default)]
    pub(crate) incarnation: Option<String>,
    /// Its picture fence, which moves on each time the window is opened
    /// with input.
    #[serde(default)]
    pub(crate) fence: Option<u64>,
}

impl Runner {
    /// Its picture fence, bound to the run that keeps it; none from an
    /// Atlas VM that keeps none.
    pub(crate) fn fence(&self) -> Option<Fence> {
        Some(Fence {
            incarnation: self.incarnation.clone()?,
            count: self.fence?,
        })
    }
}

/// What tells whether a VM's window was opened between two proofs: Atlas
/// VM's run and its picture fence. Two proofs that show the same, each with
/// the window closed, saw no opening between them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fence {
    pub(crate) incarnation: String,
    pub(crate) count: u64,
}

/// A keeper's observation of one generation of the VM's session.
#[derive(Clone, Debug)]
pub(crate) struct Observed {
    pub(crate) generation: u64,
    /// The guest key the generation's handshake pinned.
    pub(crate) peer: PeerKey,
    /// The station's own key.
    pub(crate) station: PeerKey,
    /// What the guest said, as one status: its hello, its input service's
    /// own hello as `inputService`, and its proof as `sessionProof`.
    pub(crate) status: Value,
    pub(crate) runner: Runner,
    /// What the guest declared in the generation it was observed in; none
    /// from a keeper that does not say.
    pub(crate) declared: Option<atlas_session::compatibility::Declared>,
}

impl Observed {
    /// The observation in the keeper's `answer` to `observe`, asked with
    /// `challenge`.
    ///
    /// # Errors
    ///
    /// Refuses an answer that is no observation, and a proof that does not
    /// answer `challenge`: one replayed from another observation included.
    pub(crate) fn decode(answer: &Value, challenge: &str) -> Result<Self> {
        let malformed = || {
            Error::new(
                Code::Protocol,
                "The VM's session keeper answered what is not an observation.",
            )
        };
        if answer["ok"] != true || answer["op"] != "observe" {
            return Err(malformed());
        }
        let generation = answer["generation"].as_u64().ok_or_else(malformed)?;
        let key = |name: &str| {
            answer[name]
                .as_str()
                .and_then(PeerKey::from_hex)
                .ok_or_else(malformed)
        };
        let (peer, station) = (key("peer")?, key("station")?);
        let said = &answer["status"];
        let hello = &said["hello"];
        if !hello.is_object() {
            return Err(malformed());
        }
        if hello["ok"] != true {
            return Err(Error::new(
                hello["code"]
                    .as_str()
                    .and_then(Code::parse)
                    .unwrap_or(Code::Transport),
                format!(
                    "The VM's guest did not say hello: {}",
                    hello["error"].as_str().unwrap_or("it gave no reason")
                ),
            ));
        }
        let proof = &said["sessionProof"];
        if proof["ok"] != true {
            return Err(Error::new(
                Code::Identity,
                format!(
                    "The VM's session could not be proved: {}",
                    proof["error"].as_str().unwrap_or("its proof did not come")
                ),
            ));
        }
        if proof["challenge"].as_str() != Some(challenge) || !proof["proof"].is_object() {
            return Err(Error::new(
                Code::Identity,
                "The independent session proof did not match its challenge",
            ));
        }
        let mut status = said["hello"].clone();
        status["inputService"] = said["inputService"].clone();
        status["sessionProof"] = proof["proof"].clone();
        let runner = serde_json::from_value(answer["runner"].clone()).unwrap_or_default();
        let declared = serde_json::from_value(answer["declared"].clone()).ok();
        Ok(Self {
            generation,
            peer,
            station,
            status,
            runner,
            declared,
        })
    }

    /// What the VM's proof is held to: its pairing `paired`, the keys its
    /// session proved, and what the station knows of it beside
    /// `observation`, its hands on while `maintained`.
    fn expect<'a>(
        &self,
        paired: &'a Paired,
        observation: &Observation,
        maintained: bool,
    ) -> MachineExpect<'a> {
        MachineExpect {
            paired,
            peer: self.peer,
            station: self.station,
            eligible: eligible(&self.runner, observation),
            hands_off: if maintained {
                Some(false)
            } else {
                hands_off(&self.runner)
            },
        }
    }

    /// The session this observation proves of `desktop`, held to its
    /// pairing `paired`. While the person's maintenance holds agents, as
    /// `maintained` says why, the person's hands are on it whatever its
    /// window says, and that is what its refusal says.
    ///
    /// # Errors
    ///
    /// What [`Desktop::prove_machine`] refuses.
    pub(crate) fn prove(
        &self,
        desktop: &Desktop,
        paired: &Paired,
        maintained: Option<Error>,
    ) -> Result<MachineSession> {
        let observation = Observation::decode(&self.status);
        let expect = self.expect(paired, &observation, maintained.is_some());
        desktop
            .prove_machine(&observation, &expect)
            .map_err(|refused| match maintained {
                Some(held) if refused.code == Code::OnConsole => held,
                _ => refused,
            })
    }
}

/// Why a VM's input does not count while Atlas in it is part way through
/// an update.
pub(crate) const MID_UPDATE: &str = "Atlas in the VM is part way through an update: its input \
                                     service runs another build than its daemon. Restart the VM \
                                     to finish it.";

/// Whether the guest's `status` shows Atlas in it part way through an
/// update: its input service answered ready, from another build than the
/// one its daemon started from.
pub(crate) fn mid_update(status: &Value) -> bool {
    let input = &status["inputService"];
    let (own, daemon) = (input["build"].as_str(), status["daemon"]["build"].as_str());
    input["ok"] == true
        && input["ready"] == true
        && own.is_some()
        && daemon.is_some()
        && own != daemon
}

/// Whether a VM is one Atlas may use: Atlas VM says it runs, seen just now,
/// and its guest says it is a virtual machine.
fn eligible(runner: &Runner, observation: &Observation) -> bool {
    runner.power == Some(Power::Running)
        && runner.confirmed == Some(true)
        && observation.in_virtual_machine() == Some(true)
}

/// Whether the person's hands are off a VM: its window is closed. Open is
/// on; a window Atlas VM does not say, as one of a build before it said so
/// does not, is unknown.
fn hands_off(runner: &Runner) -> Option<bool> {
    match runner.window? {
        Window::Closed => Some(true),
        Window::Open => Some(false),
        Window::Unknown => None,
    }
}

#[cfg(test)]
#[path = "proof_tests.rs"]
mod tests;
