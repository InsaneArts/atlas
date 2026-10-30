//! What an app draws at the menu layer, found by what lies under a few
//! points of each window there, and which menus met inside another window
//! or menu are left out of the scan.
use super::{Frame, batch::Source, ffi, scan::Scan};
use objc2_application_services::AXUIElement;
use objc2_core_foundation::CFRetained;

impl Scan {
    /// What the app draws in `bounds`, one of its windows at the menu layer:
    /// the element above a point inside it whose frame is the window's (an
    /// open menu, a popover, a combo box's list, a panel raised to the menu
    /// layer). A window whose points all reach a window in `read` instead
    /// shows nothing of its own (a tooltip, a click-through overlay, a menu
    /// fading out); the elements hit there must turn up in that window's
    /// scan. Menus are found this way because a context menu hangs below the
    /// view it opened from, and a menu bar or status item menu below its
    /// item, never among the app's windows.
    pub(super) fn drawn_at(
        &mut self,
        app: &AXUIElement,
        bounds: &Frame,
        read: &[Frame],
    ) -> Option<Drawn> {
        let points = [0.5, 0.25, 0.75];
        let mut hits = Vec::new();

        // A point can fall on a separator; a menu has items around it.
        for fraction in points {
            if !self.open() {
                return None;
            }

            let (x, y) = (
                bounds.x + bounds.width / 2.0,
                bounds.y + bounds.height * fraction,
            );
            let leaf = match ffi::element_at(app, x as f32, y as f32) {
                Ok(Some(leaf)) => leaf,
                Ok(None) => continue,
                Err(status) => {
                    self.failed(status, "AXElementAtPosition");
                    continue;
                }
            };

            let mut element = Some(leaf.clone());

            // An item sits a few levels below its menu or window.
            for _ in 0..12 {
                let Some(current) = element else {
                    break;
                };

                let frame = self.frame(Source::Element(&current));
                let menu =
                    self.text(Source::Element(&current), "AXRole").as_deref() == Some("AXMenu");
                let parent = self
                    .attribute(Source::Element(&current), "AXParent")
                    .and_then(|parent| parent.downcast::<AXUIElement>().ok());
                let top = parent.as_ref().is_some_and(|parent| {
                    self.text(Source::Element(parent), "AXRole").as_deref() == Some("AXApplication")
                });
                match judge(menu, frame.as_ref(), top, bounds, read) {
                    Step::Found { menu: true } => return Some(Drawn::Menu(current)),
                    Step::Found { menu: false } => return Some(Drawn::Window(current)),
                    Step::Through => {
                        hits.push(leaf);
                        break;
                    }
                    Step::Stop => break,
                    Step::Up => element = parent,
                }
            }
        }

        (hits.len() == points.len()).then_some(Drawn::Through(hits))
    }
}

/// What an app draws at the menu layer (see [`Scan::drawn_at`]).
pub(super) enum Drawn {
    Menu(CFRetained<AXUIElement>),
    Window(CFRetained<AXUIElement>),
    /// Nothing of its own: every point reached a window read already, at
    /// these elements, which that window's scan must contain.
    Through(Vec<CFRetained<AXUIElement>>),
}

/// One step up from the element hit inside a menu-layer window.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// This element has the window's frame: what it draws (a menu or not).
    Found { menu: bool },
    /// The top of another window already read: the point fell through.
    Through,
    /// The top of something else: the window cannot be placed.
    Stop,
    /// Keep climbing.
    Up,
}

/// Judge one element on the way up from a hit: whether it is `menu`, its
/// `frame`, and whether it is `top` (its parent is the application).
pub(super) fn judge(
    menu: bool,
    frame: Option<&Frame>,
    top: bool,
    bounds: &Frame,
    read: &[Frame],
) -> Step {
    if frame.is_some_and(|frame| frame.matches(bounds)) {
        return Step::Found { menu };
    }

    if !top {
        return Step::Up;
    }

    if frame.is_some_and(|frame| read.iter().any(|other| other.matches(frame))) {
        Step::Through
    } else {
        Step::Stop
    }
}

/// Whether a menu met inside a window or another menu is left out of the
/// scan: closed (no frame, or no size), or open and read as a menu of its
/// own. Any other menu, such as one a view draws itself, is read.
pub(super) fn skip_nested_menu(frame: Option<&Frame>, open: &[Frame]) -> bool {
    frame.is_none_or(|frame| {
        frame.width <= 0.0 || frame.height <= 0.0 || open.iter().any(|menu| menu.matches(frame))
    })
}
