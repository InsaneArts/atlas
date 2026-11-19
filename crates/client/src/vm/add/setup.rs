//! The add's setup window. Once the guest is closed, the VM's window opens
//! on this Mac's screen with the person's input, for them to allow Atlas
//! Screen & System Audio Recording and Accessibility as macOS asks in the
//! guest. The add waits for both through the guest's own hello, read
//! through its keeper, riding the restart that takes Screen Recording. Then
//! its window-on is settled for good, its input is turned off, and the
//! setup is done only on the proof the window gives. The VM is still being
//! added all the while: no agent reaches it until the add's own Resume
//! ([`super::resume`]), or the person's, ends the add.
use super::{Adding, Means, Posture};
use crate::vm::{
    guest_words,
    lifecycle::{
        Lifecycle,
        record::{Ended, Record, Setup, Step},
    },
    runner::{Operation, Outcome, Reply, Window},
    window::{self, Asked, AtlasVm, Proof},
};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde_json::json;
use std::time::{Duration, Instant};

/// What the add says once its setup is done.
pub(crate) const READY: &str =
    "Setup is done. Resume agents from the Atlas menu bar to let them use the VM.";
/// What the person does once it is.
pub(super) const RESUME: &str = "Resume agents from the Atlas menu bar to let them use the VM.";
/// What the add says as the window opens.
const OPENING: &str = "Opening the VM's window. In it, allow Screen & System Audio Recording and \
                       Accessibility for Atlas.";
/// What the add says once both are allowed.
const CLOSING: &str = "Both are allowed. Closing the window's input.";
/// What to run to go on.
const AGAIN: &str = "Run atlas desktop add vm again to go on.";
/// Atlas VM's word that it shows the administrator's name and password in
/// a setup window.
const WINDOW_SETUP: &str = "window-setup";

/// The guest's own word that both are allowed: half an hour from the
/// first look, however long each look takes, a look every 2 seconds.
const ALLOWING: window::Patience = window::Patience {
    within: Duration::from_secs(30 * 60),
    every: Duration::from_secs(2),
};
/// A window change, followed until Atlas VM says what it came to.
const WINDOWING: window::Patience = window::Patience {
    within: Duration::from_secs(60),
    every: Duration::from_secs(1),
};

/// Atlas VM as the add's means reach it.
struct OnVm<'a, M> {
    means: &'a M,
    lifecycle: &'a Lifecycle,
    vm: DesktopId,
}

impl<M: Means> AtlasVm for OnVm<'_, M> {
    fn change(
        &self,
        record: &mut Record,
        operation: &Operation,
        _within: Duration,
    ) -> Result<Reply> {
        let keep = |record: &Record| self.lifecycle.keep_record(record);
        self.means.change(&self.vm, record, &keep, operation)
    }

    fn read(&self, operation: &Operation) -> Result<Reply> {
        self.means.read(&self.vm, operation)
    }

    fn interrupted(&self) -> Result<()> {
        Ok(())
    }

    fn pause(&self, how_long: Duration) {
        self.means.wait(how_long);
    }

    fn now(&self) -> Instant {
        self.means.now()
    }
}

impl<'a, M: Means> Adding<'a, M> {
    /// The setup window opened, both permissions allowed in it, and its
    /// input confirmed off, each kept once done; then the add's own Resume.
    /// Whether that Resume let agents go on, which ends the add; else it is
    /// set up, for the person's Resume to end.
    pub(super) fn set_up(&mut self) -> Result<bool> {
        if !self.record.has(Step::Ready) && !self.record.has(Step::Hidden) {
            if !self.record.has(Step::Allowed) {
                self.shown()?;
                self.allowed()?;
            }
            self.hidden()?;
        }
        self.resumed_itself(self.granted)
    }

    /// Atlas VM, reached through the add's means.
    fn atlas(&self) -> OnVm<'a, M> {
        OnVm {
            means: self.means,
            lifecycle: self.lifecycle,
            vm: self.record.id.clone(),
        }
    }

    /// The setup window as the record keeps it, kept from now.
    pub(super) fn setup(&mut self) -> &mut Setup {
        self.record.setup.get_or_insert_with(Setup::default)
    }

    /// The window open with the person's input: a window-on kept is asked
    /// again as itself, as it may be on its way; one the person closed
    /// since it opened is asked anew.
    fn shown(&mut self) -> Result<()> {
        if self.record.has(Step::Shown) {
            // Open still, or it cannot be told: the person goes on in it.
            if self.window_shown() != Some(false) {
                return Ok(());
            }
            self.record.undo(Step::Shown);
            self.setup().open = None;
            self.keep()?;
        }
        if !self
            .hello
            .as_ref()
            .is_some_and(|hello| hello.can("window-input"))
        {
            return Err(Error::new(
                Code::Update,
                "This Atlas VM cannot open the VM's window; update Atlas, then run atlas \
                 desktop add vm again.",
            ));
        }
        self.say(OPENING);
        let open = self.setup().open.get_or_insert_with(Token::new).clone();
        self.keep()?;
        let uncertain = self.record.sent(&open).is_some();
        // Marked as the add's setup window for an Atlas VM that shows its
        // administrator then; one an earlier build kept is asked as kept.
        let marked = self
            .hello
            .as_ref()
            .is_some_and(|hello| hello.can(WINDOW_SETUP));
        let setup = |setup| Operation::Window {
            change: open.clone(),
            input: true,
            setup,
        };
        let operation = match self.record.sent(&open) {
            Some(kept) if kept.request != setup(marked).digest() => setup(!marked),
            _ => setup(marked),
        };
        let atlas = self.atlas();
        match window::ask(&atlas, &mut self.record, &operation, uncertain, WINDOWING)? {
            Asked::Came {
                outcome: Outcome::Done,
                by,
                ..
            } => {
                let setup = self.setup();
                if setup.opened_by.is_none() {
                    setup.opened_by = by;
                }
                self.done(Step::Shown)
            }
            Asked::Came {
                outcome: Outcome::Refused { reason },
                ..
            }
            | Asked::NotTaken(reason) => {
                self.setup().open = None;
                self.keep()?;
                Err(Error::new(
                    Code::State,
                    format!("Atlas VM did not open the VM's window: {reason}"),
                )
                .with("next", json!(AGAIN)))
            }
            // Kept, to be asked again as itself.
            Asked::Unknown => Err(Error::new(
                Code::NotReady,
                format!("Atlas VM has not said whether it opened the VM's window. {AGAIN}"),
            )),
        }
    }

    /// Screen Recording and Accessibility both allowed, as the guest's own
    /// hello says: what it says it waits for is kept and said as it changes.
    /// A guest that does not answer, as while it restarts to take Screen
    /// Recording, is waited for. The wait is bounded by the time gone by
    /// since its first look, on the means' monotonic clock, so slow replies
    /// never stretch it: no look starts once the next would be past it.
    fn allowed(&mut self) -> Result<()> {
        let selected = self.selected()?;
        let vm = self.vm();
        let began = self.means.now();
        loop {
            if let Ok(posture) = self.means.posture(&selected, &vm) {
                if posture.allowed() {
                    self.setup().waiting = None;
                    return self.done(Step::Allowed);
                }
                self.seen(&posture)?;
            }
            if self.window_shown() == Some(false) {
                return Err(Error::new(
                    Code::NotReady,
                    "The VM's window was closed before Atlas was allowed in it. Run atlas \
                     desktop add vm again to open it again.",
                ));
            }
            let spent = self.means.now().saturating_duration_since(began);
            if spent + ALLOWING.every >= ALLOWING.within {
                break;
            }
            self.means.wait(ALLOWING.every);
        }
        Err(Error::new(
            Code::NotReady,
            format!("Atlas is not allowed in the VM yet; its window stays open. {AGAIN}"),
        ))
    }

    /// Keep which of the two the guest says `posture` allows, and keep and
    /// say what it waits for, in its own words, each when it changed.
    fn seen(&mut self, posture: &Posture) -> Result<()> {
        let words = guest_words::said_of(&posture.waiting.join(" "), &self.record.name);
        let words = (!words.is_empty()).then_some(words);
        let recording = posture.recording == Some(true);
        let accessibility = posture.accessibility == Some(true);
        let setup = self.setup();
        let changed = setup.waiting != words;
        if !changed && setup.recording == recording && setup.accessibility == accessibility {
            return Ok(());
        }
        setup.waiting.clone_from(&words);
        setup.recording = recording;
        setup.accessibility = accessibility;
        self.keep()?;
        if changed && let Some(words) = words {
            self.say(&format!("In the VM: {words}"));
        }
        Ok(())
    }

    /// The window's input confirmed off: its window-on settled for good
    /// first, then turned off, its proof kept before the step.
    fn hidden(&mut self) -> Result<()> {
        self.say(CLOSING);
        if self.setup().ended.is_none() {
            if !self.setup().settled {
                self.settled()?;
            }
            let ended = self.turned_off()?;
            self.setup().ended = Some(ended);
            self.keep()?;
        }
        self.done(Step::Hidden)
    }

    /// Settle the window-on for good: Atlas VM answers what it came to, or
    /// voids it, never having seen it, so one still on its way is never
    /// done; the run that did it is its opener when none is known.
    fn settled(&mut self) -> Result<()> {
        if let Some(open) = self.setup().open.clone() {
            if !self.hello.as_ref().is_some_and(|hello| hello.can("void")) {
                return Err(Error::new(
                    Code::Update,
                    "This Atlas VM cannot make sure its request to open the VM's window is \
                     never done; update Atlas, then run atlas desktop add vm again.",
                ));
            }
            let voided = window::voided(&self.atlas(), &open, WINDOWING)?;
            let by = match voided.map(|reply| (reply.outcome, reply.by)) {
                Some((Some(Outcome::Done), Some(by))) => Some(by),
                Some((Some(Outcome::Refused { .. }), _)) => None,
                _ => {
                    return Err(Error::new(
                        Code::NotReady,
                        format!(
                            "Atlas VM has not said what the request to open the VM's window \
                             came to, so it may still take your input. {AGAIN}"
                        ),
                    ));
                }
            };
            let setup = self.setup();
            if setup.opened_by.is_none() {
                setup.opened_by = by;
            }
        }
        let setup = self.setup();
        setup.settled = true;
        setup.close = None;
        self.keep()
    }

    /// Ask Atlas VM to turn off the window's input under the change kept
    /// for it, or a new one, until a run whose word ends the window
    /// confirmed it: that proof. A window-off done by a run gone since is
    /// asked once more, of the run answering now, under a change of its
    /// own.
    fn turned_off(&mut self) -> Result<Ended> {
        let atlas = self.atlas();
        for _ in 0..2 {
            let close = self.setup().close.get_or_insert_with(Token::new).clone();
            self.keep()?;
            let uncertain = self.record.sent(&close).is_some();
            let operation = Operation::Window {
                change: close.clone(),
                input: false,
                setup: false,
            };
            match window::ask(&atlas, &mut self.record, &operation, uncertain, WINDOWING)? {
                Asked::Came {
                    outcome: Outcome::Done,
                    by,
                    answering,
                } => {
                    let opener = self.setup().opened_by.clone();
                    let proof = window::proof(
                        &atlas,
                        opener.as_deref(),
                        &close,
                        by,
                        answering.as_deref(),
                        WINDOWING,
                    )?;
                    match proof {
                        Proof::Ended(ended) => return Ok(ended),
                        Proof::Again => self.setup().close = None,
                        Proof::AnotherRun => return Err(another_run()),
                        Proof::NotAnswering => return Err(not_answering()),
                        Proof::CannotProve => {
                            return Err(Error::new(
                                Code::Update,
                                "This Atlas VM is from an older Atlas and cannot prove its \
                                 earlier run's window gone. Update Atlas, then run atlas desktop \
                                 add vm again.",
                            ));
                        }
                    }
                }
                Asked::Came {
                    outcome: Outcome::Refused { reason },
                    ..
                }
                | Asked::NotTaken(reason) => {
                    // The next add asks under a change of its own.
                    self.setup().close = None;
                    self.keep()?;
                    return Err(Error::new(
                        Code::State,
                        format!("Atlas VM could not turn off the VM's window: {reason}"),
                    )
                    .with("next", json!(AGAIN)));
                }
                // Kept, to be asked again as itself.
                Asked::Unknown => return Err(not_answering()),
            }
        }
        self.keep()?;
        Err(another_run())
    }

    /// Whether the VM's window is open now, as Atlas VM says; none when it
    /// does not say.
    fn window_shown(&self) -> Option<bool> {
        let state = self
            .means
            .read(&self.record.id, &Operation::State)
            .ok()
            .filter(|state| state.ok)?;
        match state.window? {
            Window::Open => Some(true),
            Window::Closed => Some(false),
            Window::Unknown => None,
        }
    }
}

/// Why the setup waits on an Atlas VM that does not answer.
fn not_answering() -> Error {
    Error::new(
        Code::NotReady,
        format!(
            "Atlas VM, which runs the VM, is not answering, so its window may still take your \
             input. {AGAIN}"
        ),
    )
}

/// Why the window's input off ends nothing yet: another run of Atlas VM
/// answered than the one that opened it, and cannot yet say it is gone.
fn another_run() -> Error {
    Error::new(
        Code::NotReady,
        "Atlas VM ran again since it opened the VM's window, and whether that window is gone \
         cannot be told yet. Run atlas desktop add vm again later to go on.",
    )
}
