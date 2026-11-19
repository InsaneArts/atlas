//! A page opened for the person, on their own lane: the browser a sign-in
//! uses, opened on the site, or on its own settings for a site's data. It
//! is kept as intended before anything runs, goes only while the owner it
//! names holds the person's control here, live, in its generation, checked
//! again right before its program starts, and runs as a command does,
//! `open` in this session, supervised, with what it came to kept before it
//! is answered. Neither the station's Stop nor the guest's own refuses or
//! ends it: the person may act on a stopped desktop. Its lane cancelled,
//! its generation ending, or the person's control released or run out
//! before it starts still does.
use super::{Asker, Gate, Machine, Reader, Writer, answer, ended, write};
use atlas_core::{
    error::Code,
    files::Token,
    protocol::{ConnectionPhase, failure},
    session::GRANT_TTL,
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::Instant,
};
use tokio::{io::AsyncReadExt, sync::watch};

/// The program that opens a page in the person's session; one that only
/// says what it was asked in a test, which must never open a browser.
#[cfg(not(test))]
const OPEN: &str = "/usr/bin/open";
#[cfg(test)]
const OPEN: &str = "/bin/echo";

/// How long `open` may run: it asks Launch Services and ends.
const OPENING_MS: u64 = 30_000;

/// The longest address a page is opened at.
const LONGEST: usize = 2048;

/// The person's request to open a page, as it is read here.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct PersonOpen {
    browser: Browser,
    url: String,
    /// The owner the person's control holds here.
    lease: Token,
}

/// The browsers a page is opened in for the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Browser {
    Chrome,
    Firefox,
}

impl Browser {
    /// The app, as `open -a` takes it.
    fn app(self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Firefox => "Firefox",
        }
    }

    /// Whether `url` is a page it opens for the person: a web site, or its
    /// own settings.
    fn opens(self, url: &str) -> bool {
        let settings = match self {
            Self::Chrome => "chrome://settings/",
            Self::Firefox => "about:preferences",
        };
        url.starts_with("https://") || url.starts_with(settings)
    }
}

impl PersonOpen {
    /// Whether `line` asks to open a page for the person: what their own
    /// lane carries beside their control of input.
    pub(super) fn asked(line: &[u8]) -> bool {
        serde_json::from_slice::<Value>(line).is_ok_and(|request| request["op"] == "person-open")
    }

    /// The request `line`, refused unless it bears the person's mark,
    /// names Google Chrome or Firefox, a web site or that browser's own
    /// settings, and the owner it goes under.
    ///
    /// # Errors
    ///
    /// The refusal to answer with, for certain: nothing was done.
    pub(super) fn of(line: &[u8]) -> std::result::Result<Self, Value> {
        let refused = || {
            failure(
                Code::BadRequest,
                "The request is not what its lane was opened for",
            )
        };
        let request: Value = serde_json::from_slice(line).map_err(|_| refused())?;
        if request["op"] != "person-open" || request["person"] != true {
            return Err(refused());
        }
        let browser = match request["app"].as_str() {
            Some("Google Chrome") => Browser::Chrome,
            Some("Firefox") => Browser::Firefox,
            _ => {
                return Err(failure(
                    Code::Input,
                    "Atlas opens Google Chrome or Firefox for this",
                ));
            }
        };
        let url = request["url"]
            .as_str()
            .filter(|url| url.len() <= LONGEST)
            .filter(|url| !url.chars().any(|c| c.is_whitespace() || c.is_control()))
            .filter(|url| browser.opens(url))
            .ok_or_else(|| {
                failure(
                    Code::Input,
                    "Atlas opens a site's page here, or the browser's own settings",
                )
            })?;
        let lease = request["lease"]
            .as_str()
            .and_then(Token::parse)
            .ok_or_else(refused)?;
        Ok(Self {
            browser,
            url: url.to_owned(),
            lease,
        })
    }

    /// What runs to open it: a web page in Chrome gets a window of its
    /// own, as a sign-in's does on Second Login; anything else opens where
    /// the browser puts it.
    fn argv(&self) -> Vec<String> {
        let app = self.browser.app().to_owned();
        let mut argv = vec![OPEN.to_owned()];
        if self.browser == Browser::Chrome && self.url.starts_with("https://") {
            argv.extend(["-n".to_owned(), "-a".to_owned(), app]);
            argv.extend(["--args".to_owned(), "--new-window".to_owned()]);
        } else {
            argv.extend(["-a".to_owned(), app, "--".to_owned()]);
        }
        argv.push(self.url.clone());
        argv
    }
}

/// The person's control of this desktop's input as their own lane carried
/// it to the input service: the owner holding it, the generation it was
/// taken in, and until when it holds at the latest, never later than the
/// service's own grant. Only an answer that said so takes or keeps it; a
/// release lets go of it before the service is asked, and a renewal refused
/// or unanswered lets go of it too.
#[derive(Default)]
pub(in crate::daemon) struct PersonGrants(Mutex<Option<Held>>);

/// The person's control as last granted.
struct Held {
    owner: Token,
    generation: u64,
    until: Instant,
}

impl PersonGrants {
    fn held(&self) -> std::sync::MutexGuard<'_, Option<Held>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Before `operation` by `owner` is asked of the input service: a
    /// release lets go at once.
    pub(super) fn asking(&self, operation: &str, owner: &Token) {
        let mut held = self.held();
        if operation == "release" && held.as_ref().is_some_and(|held| held.owner == *owner) {
            *held = None;
        }
    }

    /// Once the input service answered `operation` by `owner` in
    /// `generation`, asked at `sent`, with `reply`: taken or kept only when
    /// it said so for certain, from when it was asked.
    pub(super) fn answered(
        &self,
        (operation, owner, generation): (&str, &Token, u64),
        sent: Instant,
        reply: &Value,
    ) {
        let certain = reply["ok"] == true && reply["uncertain"] != true;
        let mut held = self.held();
        let theirs = held
            .as_ref()
            .is_some_and(|held| held.owner == *owner && held.generation == generation);
        match operation {
            "acquire" if certain => {
                *held = Some(Held {
                    owner: owner.clone(),
                    generation,
                    until: sent + GRANT_TTL,
                });
            }
            "renew" if certain && theirs => {
                if let Some(held) = held.as_mut() {
                    held.until = sent + GRANT_TTL;
                }
            }
            "renew" | "release" if theirs => *held = None,
            _ => {}
        }
    }

    /// Whether `owner` holds the person's control in `generation` now.
    fn holds(&self, owner: &Token, generation: u64) -> bool {
        self.held().as_ref().is_some_and(|held| {
            held.owner == *owner && held.generation == generation && Instant::now() < held.until
        })
    }
}

/// The person's control a page of theirs goes under: refused once it no
/// longer holds, right up to the word that starts the page's program.
#[derive(Clone)]
pub(in crate::daemon) struct PersonLease {
    grants: Arc<PersonGrants>,
    owner: Token,
    generation: u64,
}

impl PersonLease {
    /// The refusal of the page now, for certain: nothing was opened.
    pub(in crate::daemon) fn refused(&self) -> Option<Value> {
        (!self.grants.holds(&self.owner, self.generation)).then(lease_lost)
    }
}

/// The refusal of a page whose person's control ended before it opened.
fn lease_lost() -> Value {
    failure(
        Code::LeaseLost,
        "The person's control of this desktop ended, so nothing was opened",
    )
}

impl Machine {
    /// Open the page the person's request `line` asks for, as the change
    /// `change` on their own lane, admitted by `gate`, which no Stop holds:
    /// kept as intended first, and answered from what it came to when asked
    /// again; refused for certain unless the owner it names holds the
    /// person's control here now, in the lane's generation, and again right
    /// before its program starts; then run as a command is, `reader` the
    /// lane's own, so the station hanging up ends it.
    pub(super) async fn person_open_lane(
        self: &Arc<Self>,
        (change, line): (Token, &[u8]),
        mut gate: Gate,
        reader: Reader,
        mut writer: Writer,
    ) -> atlas_core::error::Result<()> {
        let opening = match PersonOpen::of(line) {
            Ok(opening) => opening,
            Err(refused) => return write(&mut writer, &refused).await,
        };
        // A change names the very change its lane was opened for.
        let request = serde_json::from_slice::<Value>(line)
            .ok()
            .filter(|request| request["change"] == json!(change));
        let Some(request) = request else {
            return write(
                &mut writer,
                &failure(
                    Code::BadRequest,
                    "The request is not what its lane was opened for",
                ),
            )
            .await;
        };
        if let Some(answered) = self.intend(&change, request).await {
            return write(&mut writer, &answered).await;
        }
        gate.person = Some(PersonLease {
            grants: self.persons.clone(),
            owner: opening.lease.clone(),
            generation: gate.authority.generation().0,
        });
        if let Some(refused) = gate.refused() {
            let refused = self.came_to(change, refused).await;
            return write(&mut writer, &refused).await;
        }
        let run = json!({"op": "run", "argv": opening.argv(), "timeoutMs": OPENING_MS,
            "change": change});
        let mut asked = serde_json::to_vec(&run)?;
        asked.push(b'\n');
        let reader: Reader = Box::new(std::io::Cursor::new(asked).chain(reader));
        let (lanes, lane) = (gate.lanes.clone(), gate.lane);
        let asker = Asker {
            controller: true,
            host: None,
            gate: Some(gate),
        };
        let phase = watch::channel(ConnectionPhase::Refusal).0;
        tokio::select! {
            served = answer(reader, writer, self.daemon.clone(), asker, phase) => served,
            () = ended(&lanes, lane) => Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "person_open_tests.rs"]
mod tests;
