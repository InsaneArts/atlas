//! The gate's look at the helper's screen: Accessibility and the window
//! list, read under one deadline before a request runs, never anything a
//! field holds but the message a Return would send.
use super::{Look, Message, Site, Target};
use crate::native::ax::{self, Aimed, Looks};
use atlas_core::{screen::elements::Reference, session::InputEpoch};
use std::time::{Duration, Instant};

/// The roles of fields that take typed text.
const TEXT_ROLES: [&str; 4] = ["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];

/// The helper's screen, as the input service sees it now.
pub(crate) struct OnScreen<'a> {
    pub(crate) looks: &'a Looks,
    pub(crate) epoch: &'a InputEpoch,
    pub(crate) deadline: Instant,
}

impl OnScreen<'_> {
    /// What is left of the deadline; none once it passed.
    fn left(&self) -> Option<Duration> {
        ax::remaining(self.deadline, Instant::now()).ok()
    }

    /// The process with keyboard focus.
    fn focused_pid(&self) -> Option<i64> {
        ax::focused_pid(self.left()?).ok()?
    }
}

impl Look for OnScreen<'_> {
    fn at_point(&mut self, x: f64, y: f64) -> Target {
        // The front ordinary window there; the list goes front to back.
        let Some(pid) = crate::native::windows::windows().ok().and_then(|windows| {
            windows
                .into_iter()
                .find(|window| window.layer == 0 && window.bounds.contains(x, y))
                .map(|window| window.pid)
        }) else {
            return Target::default();
        };
        let aimed = self
            .left()
            .and_then(|left| ax::pointed_aim(pid, x, y, left).ok().flatten());
        target(pid, aimed)
    }

    fn focused(&mut self) -> Target {
        let Some(pid) = self.focused_pid() else {
            return Target::default();
        };
        let aimed = self
            .left()
            .and_then(|left| ax::focused_aim(pid, left).ok().flatten());
        target(pid, aimed)
    }

    fn referenced(&mut self, reference: &Reference) -> Target {
        let Some((pid, role, name, in_page)) = self.looks.described(*reference, self.epoch) else {
            return Target::default();
        };
        Target {
            app: crate::native::apps::bundle_id(pid),
            // Its page isn't read again: an unread page takes the strictest
            // level its browser's sites have.
            site: if in_page { Site::Unread } else { Site::None },
            editable: role
                .as_deref()
                .is_some_and(|role| TEXT_ROLES.contains(&role)),
            multiline: role.as_deref() == Some("AXTextArea"),
            role,
            name,
            secure: false,
        }
    }

    fn message(&mut self) -> Message {
        let text = self
            .focused_pid()
            .zip(self.left())
            .and_then(|(pid, left)| ax::focused_text(pid, left));
        text.map_or(Message::Unread, |text| Message::whole(&text))
    }
}

/// The target in app `pid` that `aimed` reads, if anything did.
fn target(pid: i64, aimed: Option<Aimed>) -> Target {
    let app = crate::native::apps::bundle_id(pid);
    let Some(aimed) = aimed else {
        return Target {
            app,
            ..Target::default()
        };
    };
    let secure = aimed.subrole.as_deref() == Some(crate::native::focus::SECURE)
        || aimed.role.as_deref() == Some(crate::native::focus::SECURE);
    let role = aimed.role.as_deref().unwrap_or_default();
    Target {
        app,
        site: match (aimed.in_page, aimed.url.as_deref()) {
            (false, _) => Site::None,
            (true, url) => url
                .and_then(crate::native::service::page_host)
                .map_or(Site::Unread, Site::Host),
        },
        editable: TEXT_ROLES.contains(&role) || secure,
        multiline: role == "AXTextArea",
        secure,
        role: aimed.role,
        name: aimed.name,
    }
}
