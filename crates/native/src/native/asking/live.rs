//! The gate's look at the helper's screen: Accessibility and the window
//! list, read under one deadline before a request runs, never anything a
//! field holds but the message a Return would send.
use super::{Look, Message, Site, Target};
use crate::native::ax::{self, Aimed, Held, Looks};
use atlas_core::{
    screen::{
        elements::{Element, Reference},
        geometry::Bounds,
        windows::{Dock, Listed, Screen, Window},
    },
    session::InputEpoch,
};
use std::time::{Duration, Instant};

/// The browsers whose pages are accounts: a page in one that can't be read
/// takes the strictest level its listed sites have.
const BROWSERS: [&str; 10] = [
    "com.google.Chrome",
    "com.google.Chrome.canary",
    "org.chromium.Chromium",
    "com.brave.Browser",
    "com.microsoft.edgemac",
    "company.thebrowser.Browser",
    "com.vivaldi.Vivaldi",
    "com.operasoftware.Opera",
    "org.mozilla.firefox",
    "com.apple.Safari",
];

/// The roles of fields that take typed text.
const TEXT_ROLES: [&str; 4] = ["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];

/// The helper's screen, as the input service sees it now.
pub(crate) struct OnScreen<'a> {
    pub(crate) looks: &'a Looks,
    pub(crate) epoch: &'a InputEpoch,
    pub(crate) deadline: Instant,
    /// The app the request names (`--app`, `--pid`, `--window`), which its
    /// preparation brings forward before the first event: what it lands in,
    /// as the check before the first event judges it.
    pub(crate) intended: Option<String>,
    /// The element keyboard focus was on when last read.
    pub(crate) held: Option<Held>,
    /// The pointed target supplied by a service policy test.
    #[cfg(test)]
    pub(crate) point: Option<Target>,
}

impl<'a> OnScreen<'a> {
    /// The screen as a check before an event reads it: what it finds there,
    /// never the app the request named in its place.
    pub(crate) fn observing(
        looks: &'a Looks,
        epoch: &'a InputEpoch,
        deadline: Instant,
        held: Option<Held>,
    ) -> Self {
        Self {
            looks,
            epoch,
            deadline,
            intended: None,
            held,
            #[cfg(test)]
            point: None,
        }
    }

    /// `read`, unless the request names another app it will bring forward:
    /// then that app, with what lies in it not yet readable.
    pub(super) fn as_intended(&self, read: Target) -> Target {
        match &self.intended {
            Some(intended) if read.app.as_ref() != Some(intended) => Target {
                site: if BROWSERS.contains(&intended.as_str()) {
                    Site::Unread
                } else {
                    Site::None
                },
                app: Some(intended.clone()),
                ..Target::default()
            },
            _ => read,
        }
    }
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
        #[cfg(test)]
        if let Some(point) = &self.point {
            return point.clone();
        }
        let display = Bounds::display(self.epoch.width, self.epoch.height);
        let Some((pid, number)) = crate::native::windows::windows().ok().and_then(|windows| {
            let window = receiving(&windows, display, x, y)?;
            // The menu bar's menus are the app's that owns it.
            let pid = if window.is_menu_bar() {
                ax::menu_bar_owner((x, y), self.deadline).ok()??
            } else {
                window.pid
            };
            Some((pid, window.number))
        }) else {
            return Target::default();
        };
        let aimed = self
            .left()
            .and_then(|left| ax::pointed_aim(pid, x, y, left).ok().flatten());
        self.as_intended(Target {
            window: Some(number),
            ..target(pid, aimed)
        })
    }

    fn focused(&mut self) -> Target {
        self.held = None;
        let Some(pid) = self.focused_pid() else {
            return self.as_intended(Target::default());
        };
        let Some((held, aimed)) = self
            .left()
            .and_then(|left| ax::focused_aim(pid, left).ok().flatten())
        else {
            return self.as_intended(target(pid, None));
        };
        self.held = Some(held);
        let window = aimed
            .window
            .as_ref()
            .and_then(|window| window_number(pid, window));
        self.as_intended(Target {
            window,
            ..target(pid, Some(aimed))
        })
    }

    fn referenced(&mut self, reference: &Reference) -> Target {
        let Some(ax::Described {
            pid,
            role,
            name,
            in_page,
        }) = self.looks.described(*reference, self.epoch)
        else {
            return Target::default();
        };
        if in_page && let Ok(aimed) = self.looks.aimed(*reference, self.epoch, self.deadline) {
            return target(pid, Some(aimed));
        }
        Target {
            app: crate::native::apps::bundle_id(pid),
            // Failed or stale page evidence keeps the strictest listed
            // browser policy; an old URL never supplies authority.
            site: if in_page { Site::Unread } else { Site::None },
            editable: role
                .as_deref()
                .is_some_and(|role| TEXT_ROLES.contains(&role)),
            multiline: role.as_deref() == Some("AXTextArea"),
            role,
            name,
            identifier: None,
            secure: false,
            window: None,
            frame: None,
        }
    }

    fn message(&mut self) -> Message {
        let text = self
            .focused_pid()
            .zip(self.left())
            .and_then(|(pid, left)| ax::focused_text(pid, left));
        text.map_or(Message::Unread, |text| Message::whole(&text))
    }

    fn unmoved(&mut self) -> bool {
        let Some(held) = self.held.clone() else {
            return false;
        };
        self.focused_pid() == Some(held.pid)
            && self
                .left()
                .is_some_and(|left| ax::still_focused(&held, left))
    }
}

/// The window a pointer event at `x`, `y` goes to among `windows`, front
/// to back, on a display of `display`: by the rule input itself is checked
/// by, the one shown on top there at any layer, an app's open menu or
/// floating panel included. The Dock only opens apps, so where it takes
/// clicks isn't read: what lies under it is judged.
pub(super) fn receiving(windows: &[Window], display: Bounds, x: f64, y: f64) -> Option<&Window> {
    let screen = Screen::new(display, Dock::Icons(Vec::new()));
    atlas_core::screen::windows::top_at(windows, x, y, &screen)
}

/// The number of app `pid`'s listed window that `window` is, read as
/// Accessibility gave it; none when no one window is it.
fn window_number(pid: i64, window: &Element) -> Option<i64> {
    let listed = crate::native::windows::windows().ok()?;
    match ax::described(window, window.frame.as_ref()?, pid, &listed) {
        Listed::One(number) => Some(number),
        Listed::Missing | Listed::Several => None,
    }
}

/// The target in app `pid` that `aimed` reads, if anything did.
fn target(pid: i64, aimed: Option<Aimed>) -> Target {
    let app = crate::native::apps::bundle_id(pid);
    let browser = app.as_deref().is_some_and(|app| BROWSERS.contains(&app));
    let Some(aimed) = aimed else {
        // A read that failed or ran out of time in a browser: its page
        // can't be told.
        return Target {
            site: if browser { Site::Unread } else { Site::None },
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
        identifier: aimed.identifier,
        window: None,
        frame: aimed.frame,
    }
}
