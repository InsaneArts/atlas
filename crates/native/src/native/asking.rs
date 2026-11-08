//! Ask before sending: whether an agent's request may go into an account the
//! person listed, decided for the whole request before its first event.
//!
//! Each account has a level. Look Only lets agents look and scroll, never
//! click or type. Ask Before Sending lets them look, click around and type
//! drafts, and asks the person once before something is sent: Return in a
//! text field, a click or press on a control named like Send or Merge, text
//! with a line break, a password field, or anything Atlas can't tell. Trusted
//! asks nothing. An account not listed is free.
//!
//! The person's own input is never judged here, and nothing a field holds is
//! read but the value a Return would send, kept only as a digest beside the
//! preview the person sees.
use atlas_core::actions::{Action, PointerPhase};
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) mod live;
mod words;

/// The longest preview of a message the person sees in an ask.
const PREVIEW: usize = 2000;

/// What the client sends with an agent's request while the person lists any
/// account: the accounts, whether the list could be read, and the grants
/// that may cover this request.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Asking {
    #[serde(default)]
    pub(crate) accounts: Vec<Listed>,
    /// The person's list could not be read: every app counts as an account
    /// that asks before sending.
    #[serde(default)]
    pub(crate) unknown: bool,
    #[serde(default)]
    pub(crate) grants: Vec<Allowance>,
    /// Judge only, sending nothing: the check a command makes before the
    /// first of several requests, so it never stops halfway.
    #[serde(default)]
    pub(crate) dry: bool,
}

/// One account the person listed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Listed {
    pub(crate) kind: Kind,
    /// A site's registrable domain, or an app's bundle identifier.
    pub(crate) id: String,
    /// What the person reads.
    #[serde(default)]
    pub(crate) name: String,
    /// For a site, the browser it was seen in.
    #[serde(default)]
    pub(crate) browser: Option<String>,
    #[serde(default)]
    pub(crate) level: Level,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Kind {
    Site,
    App,
}

/// Which account: a site in a browser, or an app. Names are only for the
/// person to read; grants and questions go by this.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountKey {
    pub(crate) kind: Kind,
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) browser: Option<String>,
}

impl AccountKey {
    /// Any account: the one an unreadable list or app stands for.
    fn any() -> Self {
        Self {
            kind: Kind::App,
            id: "*".to_owned(),
            browser: None,
        }
    }
}

/// What a once grant is bound to where the request lands: the app, page,
/// window and element, as read before it ran.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Bound {
    pub(crate) app: Option<String>,
    pub(crate) site: Option<String>,
    pub(crate) window: Option<i64>,
    pub(crate) role: Option<String>,
    pub(crate) name: Option<String>,
}

/// How agents may act in an account, strictest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Level {
    LookOnly,
    #[default]
    Ask,
    Trusted,
}

/// What the person allowed: the rest of a task in an account, or this one
/// request once, bound to what it would send.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "scope")]
pub(crate) enum Allowance {
    /// Everything this agent session sends in the account, until it ends.
    Task { account: AccountKey },
    /// The rest of one command the person allowed once, as its first check
    /// asked: the client attaches it to that command's own requests only.
    Command { account: AccountKey },
    /// This very request, landing where it was asked about: its actions'
    /// digest, the receiving window and element, and the digest of the
    /// message a Return would send when there is one.
    Once {
        account: AccountKey,
        batch: String,
        target: Bound,
        #[serde(default)]
        value: Option<String>,
    },
}

/// Where an action lands, as read just before the request runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Target {
    /// The receiving app's bundle identifier; none when it can't be read.
    pub(crate) app: Option<String>,
    pub(crate) site: Site,
    /// The receiving window's number, when it is known.
    pub(crate) window: Option<i64>,
    /// The element's role, such as `AXButton`.
    pub(crate) role: Option<String>,
    /// The element's name; none when it has none or it can't be read.
    pub(crate) name: Option<String>,
    /// Whether keyboard focus is in an editable text element.
    pub(crate) editable: bool,
    /// Whether that element takes several lines.
    pub(crate) multiline: bool,
    /// Whether it is a password field.
    pub(crate) secure: bool,
}

/// The page an action lands in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Site {
    /// Not a web page.
    #[default]
    None,
    /// A page, by its host.
    Host(String),
    /// A browser page whose address can't be read.
    Unread,
}

/// The message a Return in a text field would send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Message {
    /// Read whole: its digest, its first [`PREVIEW`] characters, its length.
    Whole {
        digest: String,
        preview: String,
        length: usize,
    },
    /// Not read whole.
    Unread,
}

impl Message {
    /// The message `text` holds, read whole.
    pub(crate) fn whole(text: &str) -> Self {
        Self::Whole {
            digest: atlas_core::digest::bytes_sha256(text.as_bytes()),
            preview: text.chars().take(PREVIEW).collect(),
            length: text.chars().count(),
        }
    }
}

/// What the gate reads of the helper, just before a request runs: the live
/// Mac, or a test's script.
pub(crate) trait Look {
    /// Where a pointer action at `x`, `y` lands: the window there, the
    /// pressable element under the point, and its page.
    fn at_point(&mut self, x: f64, y: f64) -> Target;
    /// Where keys and typing go: the focused element and its page.
    fn focused(&mut self) -> Target;
    /// The referenced element an action acts on.
    fn referenced(&mut self, reference: &atlas_core::screen::elements::Reference) -> Target;
    /// The message a Return in the focused, non-secure field would send.
    fn message(&mut self) -> Message;
}

/// What the gate decided of a request.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decision {
    /// It may run.
    Free,
    /// The person is asked first; nothing is sent.
    Ask(Box<Question>),
    /// The account is Look Only; nothing is sent.
    Deny { account: String },
}

/// What the person is asked: the account, what the agent would do there,
/// and what binds a once grant to it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Question {
    pub(crate) account: String,
    pub(crate) account_key: AccountKey,
    pub(crate) bound: Bound,
    pub(crate) app: Option<String>,
    pub(crate) site: Option<String>,
    /// What it would do: `send-key`, `click`, `newline`, `password`,
    /// `unnamed`, `drag` or `unsure`.
    pub(crate) what: &'static str,
    pub(crate) role: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) key: Option<String>,
    /// Characters of text it would type.
    pub(crate) typed: usize,
    /// The digest of the request's actions.
    pub(crate) batch: String,
    /// The message a Return would send, read whole or not.
    pub(crate) message: Option<Message>,
}

impl Question {
    /// The question as the refusal carries it. The message's preview goes
    /// only to the person's ask, and only Just This Once binds its digest;
    /// Just This Once isn't offered when it couldn't be read whole.
    pub(crate) fn details(&self) -> Value {
        let digest = match &self.message {
            Some(Message::Whole { digest, .. }) => Some(digest.clone()),
            Some(Message::Unread) | None => None,
        };
        let mut details = json!({
            "account": self.account,
            "accountKey": self.account_key,
            "bound": self.bound,
            // What makes it this question, which a rerun joins: the account,
            // the request, where it lands and the message it would send.
            "bind": {
                "accountKey": self.account_key,
                "batch": self.batch,
                "bound": self.bound,
                "value": digest,
            },
            "app": self.app,
            "site": self.site,
            "what": self.what,
            "role": self.role,
            "name": self.name,
            "key": self.key,
            "typed": self.typed,
            "batch": self.batch,
            "once": !matches!(self.message, Some(Message::Unread)),
        });
        if let Some(Message::Whole {
            digest,
            preview,
            length,
        }) = &self.message
        {
            details["message"] = json!({"digest": digest, "preview": preview, "length": length});
        }
        details
    }
}

/// Decide the whole request `actions` against `asking`, reading where each
/// action lands through `look` before anything is sent.
#[cfg(test)]
pub(crate) fn decide(actions: &[Action], asking: &Asking, look: &mut impl Look) -> Decision {
    decide_ranked(actions, asking, look).0
}

/// How strict an account's level is: what the check before each event
/// compares, so it stops when an action would now land somewhere stricter.
fn rank(level: Option<Level>) -> u8 {
    match level {
        None | Some(Level::Trusted) => 0,
        Some(Level::Ask) => 1,
        Some(Level::LookOnly) => 2,
    }
}

/// The check before each event of a request that passed: where each action
/// lands is read again just before it is sent, and one landing in a
/// stricter account than the request was judged by stops the rest.
#[derive(Debug, Clone)]
pub(crate) struct Recheck {
    pub(crate) asking: Asking,
    ranks: std::collections::VecDeque<u8>,
    pub(crate) intended: Option<String>,
}

impl Recheck {
    /// Check `action`, the next one, by what `look` reads now: the stricter
    /// account's name when it would land in one.
    pub(crate) fn check(
        &mut self,
        action: &Action,
        look: &mut impl Look,
    ) -> std::result::Result<(), String> {
        let before = self.ranks.pop_front().unwrap_or(0);
        let target = target(action, look);
        let found = account(&target, &self.asking);
        let now = rank(found.as_ref().map(|(_, _, level)| *level));
        match found {
            Some((_, name, _)) if now > before => Err(name),
            _ => Ok(()),
        }
    }
}

/// [`decide`], with the check before each event of a request that passed.
pub(crate) fn recheck(
    actions: &[Action],
    asking: &Asking,
    look: &mut impl Look,
    intended: Option<String>,
) -> (Decision, Recheck) {
    let (decision, ranks) = decide_ranked(actions, asking, look);
    let recheck = Recheck {
        asking: asking.clone(),
        ranks: ranks.into(),
        intended,
    };
    (decision, recheck)
}

/// [`decide`], with the rank of the account each action was judged in.
fn decide_ranked(actions: &[Action], asking: &Asking, look: &mut impl Look) -> (Decision, Vec<u8>) {
    let mut ranks = Vec::with_capacity(actions.len());
    let decision = decide_into(actions, asking, look, &mut ranks);
    (decision, ranks)
}

fn decide_into(
    actions: &[Action],
    asking: &Asking,
    look: &mut impl Look,
    ranks: &mut Vec<u8>,
) -> Decision {
    let batch = digest(actions);
    // An earlier action may move focus or what lies under a point, so what
    // was read for later ones may not hold: those are "can't tell".
    let mut changed = false;
    // A pointer press decides; its release in the same request follows it.
    let mut pressed = false;
    for action in actions {
        let target = target(action, look);
        let Some((key, account, level)) = account(&target, asking) else {
            ranks.push(0);
            changed |= changes(action);
            continue;
        };
        ranks.push(rank(Some(level)));
        match level {
            Level::Trusted => {}
            Level::LookOnly if mutates(action) => return Decision::Deny { account },
            Level::LookOnly => {}
            Level::Ask => {
                let what = asks(action, &target, changed, pressed);
                if let Some(what) = what {
                    let message = (what == "send-key" && target.editable && !target.secure)
                        .then(|| look.message());
                    let bound = bound(&target);
                    if !allowed(asking, &key, &batch, &bound, message.as_ref()) {
                        return Decision::Ask(Box::new(question(
                            action,
                            &target,
                            what,
                            (key, account),
                            batch,
                            message,
                        )));
                    }
                }
            }
        }
        pressed |= matches!(
            action,
            Action::Pointer {
                phase: PointerPhase::Down,
                ..
            }
        );
        changed |= changes(action);
    }
    Decision::Free
}

/// The digest of a request's actions, which binds a once grant to it: the
/// same input service reads it back when the request comes again.
fn digest(actions: &[Action]) -> String {
    atlas_core::digest::bytes_sha256(format!("{actions:?}").as_bytes())
}

/// Where `action` lands, read through `look`.
fn target(action: &Action, look: &mut impl Look) -> Target {
    match action {
        Action::Pointer { x, y, .. }
        | Action::Move { x, y }
        | Action::Click { x, y, .. }
        | Action::Scroll { x, y, .. } => look.at_point(*x, *y),
        Action::Drag { from_x, from_y, .. } => look.at_point(*from_x, *from_y),
        Action::Type { .. } | Action::Key { .. } | Action::Wait { .. } => look.focused(),
        Action::Reference { reference, .. }
        | Action::Press { reference }
        | Action::SetValue { reference, .. }
        | Action::Check { reference }
        | Action::Uncheck { reference }
        | Action::Pick { reference, .. }
        | Action::Expand { reference }
        | Action::Collapse { reference }
        | Action::ScrollIntoView { reference } => look.referenced(reference),
    }
}

/// The listed account `target` is in, by name, and its level; none when it
/// is in none. A site counts before its browser app. A browser page whose
/// address can't be read takes the strictest level of the sites listed for
/// that browser, so Look Only is never weakened to asking.
fn account(target: &Target, asking: &Asking) -> Option<(AccountKey, String, Level)> {
    if asking.unknown {
        return Some((AccountKey::any(), "this account".to_owned(), Level::Ask));
    }
    // An app that can't be read could be any listed account: the strictest.
    let Some(app) = target.app.as_deref() else {
        return asking
            .accounts
            .iter()
            .min_by_key(|listed| listed.level)
            .map(|listed| (AccountKey::any(), "this account".to_owned(), listed.level));
    };
    let app = Some(app);
    let in_browser =
        |listed: &&Listed| listed.browser.is_none() || listed.browser.as_deref() == app;
    let sites = asking
        .accounts
        .iter()
        .filter(|listed| listed.kind == Kind::Site)
        .filter(in_browser);
    let site = match &target.site {
        Site::Host(host) => sites
            .filter(|listed| host == &listed.id || host.ends_with(&format!(".{}", listed.id)))
            .min_by_key(|listed| listed.level),
        Site::Unread => sites.min_by_key(|listed| listed.level),
        Site::None => None,
    };
    let app_listed = || {
        asking
            .accounts
            .iter()
            .filter(|listed| listed.kind == Kind::App && Some(listed.id.as_str()) == app)
            .min_by_key(|listed| listed.level)
    };
    site.or_else(app_listed).map(|listed| {
        let key = AccountKey {
            kind: listed.kind,
            id: listed.id.clone(),
            browser: listed.browser.clone(),
        };
        (key, display(listed), listed.level)
    })
}

/// What the person reads of an account.
fn display(listed: &Listed) -> String {
    if listed.name.is_empty() {
        listed.id.clone()
    } else {
        listed.name.clone()
    }
}

/// Whether `action` changes anything in the helper: what Look Only refuses.
fn mutates(action: &Action) -> bool {
    match action {
        Action::Pointer { phase, .. } => !matches!(phase, PointerPhase::Move),
        Action::Move { .. }
        | Action::Scroll { .. }
        | Action::Wait { .. }
        | Action::ScrollIntoView { .. } => false,
        Action::Click { .. }
        | Action::Drag { .. }
        | Action::Type { .. }
        | Action::Key { .. }
        | Action::Reference { .. }
        | Action::Press { .. }
        | Action::SetValue { .. }
        | Action::Check { .. }
        | Action::Uncheck { .. }
        | Action::Pick { .. }
        | Action::Expand { .. }
        | Action::Collapse { .. } => true,
    }
}

/// Whether `action` may move focus or change what lies under a point, so
/// what was read for the actions after it may no longer hold.
fn changes(action: &Action) -> bool {
    match action {
        Action::Pointer { phase, .. } => !matches!(phase, PointerPhase::Move),
        Action::Move { .. }
        | Action::Scroll { .. }
        | Action::Wait { .. }
        | Action::Type { .. }
        | Action::ScrollIntoView { .. } => false,
        Action::Click { .. }
        | Action::Drag { .. }
        | Action::Key { .. }
        | Action::Reference { .. }
        | Action::Press { .. }
        | Action::SetValue { .. }
        | Action::Check { .. }
        | Action::Uncheck { .. }
        | Action::Pick { .. }
        | Action::Expand { .. }
        | Action::Collapse { .. } => true,
    }
}

/// Why `action` asks in an account that asks before sending, or none when
/// it is free there. `changed` says an earlier action may have moved what
/// was read; `pressed` that a press in this request already decided for its
/// release.
fn asks(action: &Action, target: &Target, changed: bool, pressed: bool) -> Option<&'static str> {
    match action {
        Action::Pointer { phase, .. } => match phase {
            PointerPhase::Move => None,
            PointerPhase::Up if pressed => None,
            PointerPhase::Down | PointerPhase::Up => by_name(target, changed),
        },
        Action::Move { .. }
        | Action::Scroll { .. }
        | Action::Wait { .. }
        | Action::ScrollIntoView { .. } => None,
        Action::Click { .. } | Action::Reference { .. } | Action::Press { .. } => {
            by_name(target, changed)
        }
        Action::Drag { .. } => Some("drag"),
        Action::Type { text } => {
            if text.contains(['\n', '\r']) {
                Some("newline")
            } else if target.secure {
                Some("password")
            } else if changed || !target.editable {
                // Typing that lands outside a text field may be a shortcut.
                Some("unsure")
            } else {
                None
            }
        }
        Action::SetValue { value, .. } if target.editable => {
            value.contains(['\n', '\r']).then_some("newline")
        }
        Action::SetValue { .. }
        | Action::Check { .. }
        | Action::Uncheck { .. }
        | Action::Pick { .. }
        | Action::Expand { .. }
        | Action::Collapse { .. } => by_name(target, changed),
        Action::Key { key } => by_key(key, target, changed),
    }
}

/// A key's verdict: navigation is free, Return sends from a text field
/// (Shift+Return only breaks a line in one that takes several), Return or
/// Space on a control goes by its name, and anything else with no text
/// field focused may be a shortcut that sends, so it asks.
fn by_key(key: &str, target: &Target, changed: bool) -> Option<&'static str> {
    let lowered = key.to_ascii_lowercase();
    let parts: Vec<&str> = lowered.split('+').collect();
    let base = parts.last().copied().unwrap_or_default();
    let shift_only = parts.len() == 2 && parts[0] == "shift";
    if words::NAVIGATION.contains(&base) {
        return None;
    }
    if changed {
        return Some("unsure");
    }
    match base {
        "return" | "enter" if target.editable => {
            (!(shift_only && target.multiline)).then_some("send-key")
        }
        "return" | "enter" | "space" => by_name(target, false),
        _ if target.editable && parts.len() == 1 => None,
        _ if target.editable && shift_only => None,
        _ => Some("unsure"),
    }
}

/// A control's verdict by its name: one named like a sending verb asks, an
/// unnamed or unread one asks, and any other is free.
fn by_name(target: &Target, changed: bool) -> Option<&'static str> {
    if changed {
        return Some("unsure");
    }
    match target.name.as_deref() {
        None => Some("unnamed"),
        Some(name) if words::sends(name) => Some("click"),
        Some(_) => None,
    }
}

/// Whether a grant in `asking` covers this request in `account`: a task
/// grant there, or a once grant for this very request and message.
fn allowed(
    asking: &Asking,
    account: &AccountKey,
    batch: &str,
    target: &Bound,
    message: Option<&Message>,
) -> bool {
    asking.grants.iter().any(|grant| match grant {
        Allowance::Task { account: granted } | Allowance::Command { account: granted } => {
            granted == account
        }
        Allowance::Once {
            account: granted,
            batch: allowed,
            target: approved,
            value,
        } => {
            let same_message = match message {
                None => true,
                Some(Message::Whole { digest, .. }) => value.as_deref() == Some(digest),
                Some(Message::Unread) => false,
            };
            granted == account && allowed == batch && approved == target && same_message
        }
    })
}

/// Where `target` lands, as a once grant is bound to it.
fn bound(target: &Target) -> Bound {
    Bound {
        app: target.app.clone(),
        site: match &target.site {
            Site::Host(host) => Some(host.clone()),
            Site::None | Site::Unread => None,
        },
        window: target.window,
        role: target.role.clone(),
        name: target.name.clone(),
    }
}

/// The question about `action`, landing at `target`.
fn question(
    action: &Action,
    target: &Target,
    what: &'static str,
    (account_key, account): (AccountKey, String),
    batch: String,
    message: Option<Message>,
) -> Question {
    let (key, typed) = match action {
        Action::Key { key } => (Some(key.clone()), 0),
        Action::Type { text } => (None, text.chars().count()),
        _ => (None, 0),
    };
    Question {
        account,
        account_key,
        bound: bound(target),
        app: target.app.clone(),
        site: match &target.site {
            Site::Host(host) => Some(host.clone()),
            Site::None | Site::Unread => None,
        },
        what,
        // A password field says only how many characters would go in.
        role: target.role.clone(),
        name: if target.secure {
            None
        } else {
            target.name.clone()
        },
        key,
        typed,
        batch,
        message,
    }
}

#[cfg(test)]
mod tests;
