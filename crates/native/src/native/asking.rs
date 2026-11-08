//! Ask before sending: whether an agent's request may go into an account the
//! person listed, decided for each action just before its first event, where
//! it lands then, and again before each further event.
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
    /// The digest of the whole text the typing command this request is
    /// part of types: what that command's grant is bound to.
    #[serde(default)]
    pub(crate) typing: Option<String>,
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
    /// Any account: the one an unreadable list stands for.
    fn any() -> Self {
        Self {
            kind: Kind::App,
            id: "*".to_owned(),
            browser: None,
        }
    }

    /// An account that can't be told: an app or a page that doesn't read.
    fn unread() -> Self {
        Self {
            kind: Kind::App,
            id: "?".to_owned(),
            browser: None,
        }
    }
}

/// The listed account a target is in.
struct Found {
    key: AccountKey,
    name: String,
    level: Level,
    /// Whether which account it is was read. An app or page that can't be
    /// told could be any listed one, and no grant for one account covers it.
    known: bool,
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
    #[serde(default)]
    pub(crate) identifier: Option<String>,
    /// Where a drag drops, bound as where it starts is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) drop: Option<Box<Bound>>,
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
    /// The text of one typing command the person allowed once, as its check
    /// asked: by the digest of all of it, typed into the field and window
    /// it was asked about. The client attaches it to that command's own
    /// requests only.
    Command {
        account: AccountKey,
        target: Bound,
        typing: String,
    },
    /// One action the person allowed after it stopped a request, landing
    /// where it was asked about: its whole payload's digest, the receiving
    /// window and element, and the digest of the message it would send.
    /// The client takes it from the person's store for one request only.
    Permit {
        account: AccountKey,
        action: String,
        target: Bound,
        #[serde(default)]
        value: Option<String>,
    },
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
    /// The element's identifier (in a page, its DOM id), when it has one.
    pub(crate) identifier: Option<String>,
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
    /// Whether keyboard focus is still on the element the last
    /// [`Look::focused`] found it on, told cheaply: then what that read
    /// still holds, since an element stays in its window and page.
    fn unmoved(&mut self) -> bool;
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
    /// The digest of the action it asks about.
    pub(crate) action: String,
    /// The message a Return would send, read whole or not.
    pub(crate) message: Option<Message>,
    /// The digest of the whole text of the typing command it is part of.
    pub(crate) typing: Option<String>,
    /// Whether Just This Once can be offered: where it lands, and the
    /// message it would send, could be bound.
    pub(crate) once: bool,
    /// Whether Allow for This Task can be offered: which account it is was
    /// read, and the person's list too.
    pub(crate) task: bool,
}

impl Question {
    /// The question as the refusal carries it. The message's preview goes
    /// only to the person's ask, and only Just This Once binds its digest.
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
            // the action, where it lands and the message it would send.
            "bind": {
                "accountKey": self.account_key,
                "action": self.action,
                "bound": self.bound,
                "value": digest,
                "typing": self.typing,
            },
            "app": self.app,
            "site": self.site,
            "what": self.what,
            "role": self.role,
            "name": self.name,
            "key": self.key,
            "typed": self.typed,
            "batch": self.batch,
            "action": self.action,
            "typing": self.typing,
            "once": self.once,
            "task": self.task,
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

/// Decide `actions` against `asking` as the screen is now, sending nothing:
/// a typing command's check before its first character.
pub(crate) fn decide(actions: &[Action], asking: &Asking, look: &mut impl Look) -> Decision {
    let batch = digest(actions);
    // A pointer press decides; its release in the same request follows it.
    let mut pressed = false;
    for action in actions {
        let decision = judge(action, asking, look, &batch, pressed);
        if decision != Decision::Free {
            return decision;
        }
        pressed = holds(action, pressed);
    }
    Decision::Free
}

/// The check before each event of a request: what each action would do is
/// judged where it lands just before its first event, and again before each
/// further one, by the grants the request brought. One they don't cover
/// stops the request there.
pub(crate) struct Recheck {
    asking: Asking,
    batch: String,
    pressed: bool,
    /// The action under way, judged again before each of its events after
    /// the first.
    current: Option<Action>,
    /// The element keyboard focus was on when last read, which tells
    /// cheaply between two characters that it hasn't moved.
    pub(crate) held: Option<crate::native::ax::Held>,
}

impl Recheck {
    /// Before `action`, the next one: judged where it lands now.
    pub(crate) fn check(&mut self, action: &Action, look: &mut impl Look) -> Decision {
        let decision = judge(action, &self.asking, look, &self.batch, self.pressed);
        self.pressed = holds(action, self.pressed);
        self.current = Some(action.clone());
        decision
    }

    /// Before the next event of the action under way. A character typed
    /// while focus stays on the element the last read found goes as that
    /// was judged; anything else is judged again where it lands now.
    pub(crate) fn event(&mut self, look: &mut impl Look) -> Decision {
        let Some(action) = &self.current else {
            return Decision::Free;
        };
        if matches!(action, Action::Type { .. }) && look.unmoved() {
            return Decision::Free;
        }
        judge(action, &self.asking, look, &self.batch, false)
    }
}

/// The check before each event of the request `actions`, judged by
/// `asking`.
pub(crate) fn recheck(actions: &[Action], asking: &Asking) -> Recheck {
    Recheck {
        asking: asking.clone(),
        batch: digest(actions),
        pressed: false,
        current: None,
        held: None,
    }
}

/// What `action` of the request whose digest is `batch` comes to where it
/// lands, read through `look`; `pressed` says a press before it holds the
/// button. A drag lands at both its ends, and the account at each must
/// allow it.
fn judge(
    action: &Action,
    asking: &Asking,
    look: &mut impl Look,
    batch: &str,
    pressed: bool,
) -> Decision {
    let target = target(action, look);
    let drop = match action {
        Action::Drag { to_x, to_y, .. } => Some(look.at_point(*to_x, *to_y)),
        _ => None,
    };
    let found: Vec<Found> = std::iter::once(&target)
        .chain(drop.as_ref())
        .filter_map(|landing| account(landing, asking))
        .collect();
    // A move with a button held, a scroll's own move included, is a drag:
    // a change where it lands.
    let changes_it = mutates(action)
        || (pressed
            && matches!(
                action,
                Action::Pointer { .. } | Action::Move { .. } | Action::Scroll { .. }
            ));
    if let Some(strict) = found.iter().find(|found| found.level == Level::LookOnly)
        && changes_it
    {
        return Decision::Deny {
            account: strict.name.clone(),
        };
    }
    let Some(what) = asks(action, &target, pressed) else {
        return Decision::Free;
    };
    // A task grant covers one account by name: never while the list can't
    // be read, when every app stands for any account.
    let tasks = !asking.unknown;
    // The ends that ask, and that no task grant for their account covers.
    let open: Vec<&Found> = found
        .iter()
        .filter(|found| found.level == Level::Ask)
        .filter(|found| {
            !(tasks
                && found.known
                && asking.grants.contains(&Allowance::Task {
                    account: found.key.clone(),
                }))
        })
        .collect();
    let Some(first) = open.first() else {
        return Decision::Free;
    };
    let known = open.iter().all(|found| found.known);
    let message = (what == "send-key" && target.editable && !target.secure).then(|| look.message());
    let pinned = known
        && pinned(action, &target)
        && drop.as_ref().is_none_or(|drop| drop.window.is_some())
        && !matches!(message, Some(Message::Unread));
    let bound = Bound {
        drop: drop.as_ref().map(|drop| Box::new(bound(drop))),
        ..bound(&target)
    };
    if pinned && allowed(asking, action, &first.key, batch, &bound, message.as_ref()) {
        return Decision::Free;
    }
    Decision::Ask(Box::new(Question {
        once: pinned,
        task: tasks && known,
        typing: asking.typing.clone(),
        bound,
        ..question(
            action,
            &target,
            what,
            (first.key.clone(), first.name.clone()),
            batch.to_owned(),
            message,
        )
    }))
}

/// Whether a press is held after `action`, when one was before it as
/// `pressed` says.
fn holds(action: &Action, pressed: bool) -> bool {
    match action {
        Action::Pointer { phase, .. } => match phase {
            PointerPhase::Down => true,
            PointerPhase::Up => false,
            PointerPhase::Move => pressed,
        },
        Action::Move { .. }
        | Action::Click { .. }
        | Action::Drag { .. }
        | Action::Scroll { .. }
        | Action::Type { .. }
        | Action::Key { .. }
        | Action::Wait { .. }
        | Action::Reference { .. }
        | Action::Press { .. }
        | Action::SetValue { .. }
        | Action::Check { .. }
        | Action::Uncheck { .. }
        | Action::Pick { .. }
        | Action::Expand { .. }
        | Action::Collapse { .. }
        | Action::ScrollIntoView { .. } => pressed,
    }
}

/// Whether a once grant can be bound to where `action` lands at `target`:
/// a referenced element by its reference, anything else by its window.
fn pinned(action: &Action, target: &Target) -> bool {
    action.reference().is_some() || target.window.is_some()
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
/// that browser, so Look Only is never weakened to asking, and which of
/// them it is stays unknown.
fn account(target: &Target, asking: &Asking) -> Option<Found> {
    let unknown = |level| Found {
        key: AccountKey::unread(),
        name: "this account".to_owned(),
        level,
        known: false,
    };
    if asking.unknown {
        return Some(Found {
            key: AccountKey::any(),
            known: true,
            ..unknown(Level::Ask)
        });
    }
    // An app that can't be read could be any listed account: the strictest.
    let Some(app) = target.app.as_deref() else {
        return asking
            .accounts
            .iter()
            .map(|listed| listed.level)
            .min()
            .map(unknown);
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
        Site::Unread => match sites.map(|listed| listed.level).min() {
            Some(level) => return Some(unknown(level)),
            None => None,
        },
        Site::None => None,
    };
    let app_listed = || {
        asking
            .accounts
            .iter()
            .filter(|listed| listed.kind == Kind::App && Some(listed.id.as_str()) == app)
            .min_by_key(|listed| listed.level)
    };
    site.or_else(app_listed).map(|listed| Found {
        key: AccountKey {
            kind: listed.kind,
            id: listed.id.clone(),
            browser: listed.browser.clone(),
        },
        name: display(listed),
        level: listed.level,
        known: true,
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

/// Why `action` asks in an account that asks before sending, or none when
/// it is free there; `pressed` says a press in this request already decided
/// for its release.
fn asks(action: &Action, target: &Target, pressed: bool) -> Option<&'static str> {
    match action {
        Action::Pointer { phase, .. } => match phase {
            PointerPhase::Move if pressed => Some("drag"),
            PointerPhase::Move => None,
            PointerPhase::Up if pressed => None,
            PointerPhase::Down | PointerPhase::Up => by_name(target),
        },
        Action::Move { .. } | Action::Scroll { .. } if pressed => Some("drag"),
        Action::Move { .. }
        | Action::Scroll { .. }
        | Action::Wait { .. }
        | Action::ScrollIntoView { .. } => None,
        Action::Click { .. } | Action::Reference { .. } | Action::Press { .. } => by_name(target),
        Action::Drag { .. } => Some("drag"),
        Action::Type { text } => {
            if text.contains(['\n', '\r']) {
                Some("newline")
            } else if target.secure {
                Some("password")
            } else if !target.editable {
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
        | Action::Collapse { .. } => by_name(target),
        Action::Key { key } => by_key(key, target),
    }
}

/// A key's verdict: navigation is free, Return sends from a text field
/// (Shift+Return only breaks a line in one that takes several), Return or
/// Space on a control goes by its name, and anything else with no text
/// field focused may be a shortcut that sends, so it asks.
fn by_key(key: &str, target: &Target) -> Option<&'static str> {
    let lowered = key.to_ascii_lowercase();
    let parts: Vec<&str> = lowered.split('+').collect();
    let base = parts.last().copied().unwrap_or_default();
    let shift_only = parts.len() == 2 && parts[0] == "shift";
    if words::NAVIGATION.contains(&base) {
        return None;
    }
    // A character typed into a password field follows the password rule.
    let character = base.chars().count() == 1 && (parts.len() == 1 || shift_only);
    if target.secure && character {
        return Some("password");
    }
    match base {
        "return" | "enter" if target.editable => {
            (!(shift_only && target.multiline)).then_some("send-key")
        }
        "return" | "enter" | "space" => by_name(target),
        _ if target.editable && parts.len() == 1 => None,
        _ if target.editable && shift_only => None,
        _ => Some("unsure"),
    }
}

/// A control's verdict by its name: one named like a sending verb asks, an
/// unnamed or unread one asks, and any other is free.
fn by_name(target: &Target) -> Option<&'static str> {
    match target.name.as_deref() {
        None => Some("unnamed"),
        Some(name) if words::sends(name) => Some("click"),
        Some(_) => None,
    }
}

/// Whether a once, permit or command grant in `asking` covers `action` of
/// this request in `account`, landing where `target` binds: a once grant
/// for this very request and message, a permit for this very action and
/// message, or a command grant for this typing into this field. Task grants
/// are each end's own, judged before.
fn allowed(
    asking: &Asking,
    action: &Action,
    account: &AccountKey,
    batch: &str,
    target: &Bound,
    message: Option<&Message>,
) -> bool {
    asking.grants.iter().any(|grant| match grant {
        Allowance::Task { .. } => false,
        Allowance::Command {
            account: granted,
            target: approved,
            typing,
        } => {
            matches!(action, Action::Type { .. })
                && asking.typing.as_ref() == Some(typing)
                && granted == account
                && target == approved
        }
        Allowance::Permit {
            account: granted,
            action: allowed,
            target: approved,
            value,
        } => {
            granted == account
                && *allowed == action.digest()
                && target == approved
                && same_message(message, value.as_deref())
        }
        Allowance::Once {
            account: granted,
            batch: allowed,
            target: approved,
            value,
        } => {
            granted == account
                && allowed == batch
                && target == approved
                && same_message(message, value.as_deref())
        }
    })
}

/// Whether `message`, what an action would send, is the one whose digest a
/// grant names as `value`: always, for an action that sends none.
fn same_message(message: Option<&Message>, value: Option<&str>) -> bool {
    match message {
        None => true,
        Some(Message::Whole { digest, .. }) => value == Some(digest.as_str()),
        Some(Message::Unread) => false,
    }
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
        identifier: target.identifier.clone(),
        drop: None,
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
        action: action.digest(),
        message,
        typing: None,
        once: false,
        task: false,
    }
}

#[cfg(test)]
mod tests;
