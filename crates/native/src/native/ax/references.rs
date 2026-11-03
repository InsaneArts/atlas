//! The looks the input service keeps: each scan of an app with the object
//! every element of it is, for one process instance in one session epoch,
//! so a reference names the object that look saw and nothing else. Looks
//! are bounded in number, size and age, and named by this service's process
//! and a count it never reuses; a reference to one no longer kept is
//! refused, never matched again by name or place. The objects stay in this
//! process: replies carry only a look's name.
use super::{
    batch::Source,
    bring::{check_application, target_application},
    ffi::{self, Kept},
    scan::Scan,
    tree::{Observed, Retraced, observe, retrace},
};
use crate::native;
use atlas_core::{
    actions::{Expect, RESOLVE},
    error::{Code, Error, Result, advice::DISABLED_CONTROL},
    process::Instance,
    screen::{
        elements::{
            Element, LOOK_AGAIN, Limits, Look, Reference, Snapshot, click_point, drawn,
            element_check, stale,
        },
        geometry::Bounds,
    },
    session::InputEpoch,
};
use objc2_application_services::AXUIElement;
use serde_json::json;
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

/// The most looks kept at once.
const KEPT: usize = 16;

/// The most elements kept across every look: four of the largest scans.
const ELEMENTS: usize = 6000;

/// How long a look is kept.
const LIFETIME: Duration = Duration::from_secs(5 * 60);

/// How many elements up from what lies at a point the referenced one may be:
/// a control's own label or icon is hit first.
const HIT_DEPTH: usize = 12;

/// One element of a look: the object it is, the index of the element it
/// was reached from, what it reported, and the identifying attributes that
/// read left unproven.
#[derive(Clone)]
struct Seen {
    object: Kept,
    parent: Option<usize>,
    element: Element,
    unproven: Vec<&'static str>,
}

/// One look: its name, the epoch and process instance it was of, when it
/// was taken, and every element it saw by index.
struct Taken {
    name: Look,
    epoch: InputEpoch,
    instance: Instance,
    at: Instant,
    seen: Vec<Seen>,
}

/// The looks kept, oldest first, and the name the next one gets; none once
/// this service has given every name it has.
struct Shelf {
    looks: VecDeque<Taken>,
    next: Option<Look>,
}

impl Shelf {
    /// Let go of every look older than [`LIFETIME`] at `now`.
    fn expire(&mut self, now: Instant) {
        self.looks
            .retain(|look| now.saturating_duration_since(look.at) < LIFETIME);
    }

    /// How many elements every look kept holds.
    fn size(&self) -> usize {
        self.looks.iter().map(|look| look.seen.len()).sum()
    }
}

/// Every look the service keeps. Its lock is held only to keep, find or let
/// go of looks, never across an Accessibility call.
pub(in crate::native) struct Looks {
    shelf: Mutex<Shelf>,
}

impl Default for Looks {
    /// The looks of this service process, whose ID begins each name, so no
    /// name another service gave can name one of them.
    fn default() -> Self {
        Self {
            shelf: Mutex::new(Shelf {
                looks: VecDeque::new(),
                next: Some(Look {
                    service: std::process::id(),
                    sequence: 0,
                }),
            }),
        }
    }
}

/// One look as read, before it is kept.
pub(in crate::native) struct Read {
    instance: Instance,
    snapshot: Snapshot,
    seen: Vec<Seen>,
}

/// What a kept look holds of one referenced element: the process it is in,
/// and the element followed by each element it was reached from, nearest
/// first.
#[derive(Clone)]
struct Found {
    instance: Instance,
    chain: Vec<Seen>,
}

/// Where a referenced element is clicked and what the click must reach,
/// with what its look kept of it, to find it again just before the click.
#[derive(Clone)]
pub(in crate::native) struct Resolved {
    pub(in crate::native) x: f64,
    pub(in crate::native) y: f64,
    pub(in crate::native) expect: Expect,
    found: Found,
}

#[cfg(test)]
impl Resolved {
    /// A resolution a stand-in engine hands back: kept of nothing.
    pub(in crate::native) fn at(x: f64, y: f64, expect: Expect) -> Self {
        Self {
            x,
            y,
            expect,
            found: Found {
                instance: Instance::new(0, [0, 0]),
                chain: Vec::new(),
            },
        }
    }
}

fn unavailable() -> Error {
    Error::new(Code::Native, "The input service's looks are unavailable")
}

impl Looks {
    /// Keep `read`, taken in `epoch`, under the next name, letting the
    /// oldest looks go while more would be kept than the bounds allow. The
    /// snapshot comes back to be listed.
    ///
    /// # Errors
    ///
    /// Refuses once this service has given every name it has.
    pub(in crate::native) fn keep(
        &self,
        epoch: InputEpoch,
        read: Read,
        now: Instant,
    ) -> Result<(Look, Snapshot)> {
        let mut shelf = self.shelf.lock().map_err(|_| unavailable())?;
        let name = shelf.next.ok_or_else(|| {
            Error::new(
                Code::Elements,
                "The input service has named every look it can; run atlas start-input",
            )
        })?;
        shelf.expire(now);
        while !shelf.looks.is_empty()
            && (shelf.looks.len() >= KEPT || shelf.size() + read.seen.len() > ELEMENTS)
        {
            shelf.looks.pop_front();
        }

        shelf.next = name.after();
        shelf.looks.push_back(Taken {
            name,
            epoch,
            instance: read.instance,
            at: now,
            seen: read.seen,
        });
        Ok((name, read.snapshot))
    }

    /// What the look `reference` names holds of its element at `now`,
    /// when that look is still kept and was taken in `epoch`.
    fn find(&self, reference: Reference, epoch: &InputEpoch, now: Instant) -> Result<Found> {
        let mut shelf = self.shelf.lock().map_err(|_| unavailable())?;
        if shelf
            .next
            .is_some_and(|next| next.service != reference.look.service)
        {
            return Err(stale("That look was taken by an earlier input service"));
        }
        shelf.expire(now);
        let look = shelf
            .looks
            .iter()
            .find(|look| look.name == reference.look)
            .ok_or_else(|| stale("That look is no longer kept"))?;
        if look.epoch != *epoch {
            return Err(stale(
                "That look was taken before the helper's display changed",
            ));
        }

        let mut at = reference.index;
        let seen = look
            .seen
            .get(at)
            .ok_or_else(|| stale("That look has no element of that number"))?;
        let mut chain = vec![seen.clone()];
        // Each element was reached from one found before it, so every step
        // goes to a smaller index and the walk up ends.
        let mut parent = seen.parent;
        while let Some(above) = parent.filter(|above| *above < at) {
            let seen = look.seen.get(above).ok_or_else(unavailable)?;
            chain.push(seen.clone());
            (at, parent) = (above, seen.parent);
        }

        Ok(Found {
            instance: look.instance,
            chain,
        })
    }
}

/// Read the elements of the app `expect` names, as `atlas elements` lists
/// them, with the object each one is. Refused when that process is gone,
/// or was replaced while it was read: the look would be of another.
///
/// # Errors
///
/// Returns the read's own failures as [`super::elements`] does.
pub(in crate::native) fn read(expect: &Expect) -> Result<Read> {
    let instance = target_application(expect).map_err(|cause| {
        Error::new(Code::Elements, "No app is running with that process ID").with_source(cause)
    })?;
    let Observed { snapshot, objects } = observe(instance.pid, Limits::default())?;
    check_application(Some(instance)).map_err(|cause| {
        Error::new(
            Code::Elements,
            "The app was replaced while it was read; look again",
        )
        .with_source(cause)
    })?;

    let seen = objects
        .into_iter()
        .zip(&snapshot.elements)
        .map(|(traced, element)| Seen {
            object: Kept(traced.object),
            parent: traced.parent,
            element: element.clone(),
            unproven: traced.unproven,
        })
        .collect();
    Ok(Read {
        instance,
        snapshot,
        seen,
    })
}

/// Where the element `reference` names is now and what a click there must
/// reach: the look still kept, of this epoch and of the app `named` when a
/// batch names one, and its element met again where the look saw it.
///
/// # Errors
///
/// Refuses, with nothing sent, a look no longer kept, of another service,
/// epoch or app, and an element [`locate`] refuses.
pub(in crate::native) fn resolve(
    looks: &Looks,
    reference: Reference,
    epoch: &InputEpoch,
    named: Option<Instance>,
) -> Result<Resolved> {
    let found = looks.find(reference, epoch, Instant::now())?;
    if named.is_some_and(|named| named != found.instance) {
        return Err(stale("That look is of another app than the one named"));
    }
    let (x, y, expect) = locate(&found, epoch)?;
    Ok(Resolved {
        x,
        y,
        expect,
        found,
    })
}

/// Whether the element `resolved` names is still the one to click, once its
/// app was brought forward and just before the click: met again where its
/// look saw it, at the same point, and what is hit there. Nothing is asked
/// to move or take focus.
///
/// # Errors
///
/// Refuses, with nothing sent, an element [`locate`] refuses, one that
/// moved, and a point where something else is hit.
pub(in crate::native) fn confirm(resolved: &Resolved, epoch: &InputEpoch) -> Result<()> {
    let (x, y, _) = locate(&resolved.found, epoch)?;
    if (x, y) != (resolved.x, resolved.y) {
        return Err(stale("The element moved while its app was brought forward"));
    }
    let target = resolved.found.chain.first().ok_or_else(unavailable)?;
    if !lands_on(resolved.found.instance.pid, (x, y), &target.object.0) {
        return Err(stale("Something else is at the element's point now"));
    }
    Ok(())
}

/// Where the element `found` names is clicked now and what the click must
/// reach: its app the same process, and its path from where a walk starts
/// read again within [`RESOLVE`], each element on it proven the same as the
/// look saw it.
///
/// # Errors
///
/// Refuses, with nothing sent, an app that quit or was replaced, an element
/// no longer where the look saw it, read too slowly, changed or not proven
/// the same, a disabled control, and an element off screen or whose window
/// cannot be told.
fn locate(found: &Found, epoch: &InputEpoch) -> Result<(f64, f64, Expect)> {
    let replaced = |cause: Error| stale("The app it is in quit or was replaced").with_source(cause);
    check_application(Some(found.instance)).map_err(replaced)?;
    let path: Vec<&AXUIElement> = found
        .chain
        .iter()
        .rev()
        .map(|seen| &*seen.object.0)
        .collect();
    let started = Instant::now();
    let retraced = retrace(found.instance.pid, &path, RESOLVE)?;
    check_application(Some(found.instance)).map_err(replaced)?;
    // A read that ended late may describe an app that has moved on since.
    if started.elapsed() > RESOLVE {
        return Err(stale(
            "The app answered too slowly to find the element again",
        ));
    }
    let retraced = retraced
        .filter(|retraced| retraced.len() == found.chain.len())
        .ok_or_else(|| stale("The element is no longer where that look saw it"))?;
    if !found
        .chain
        .iter()
        .rev()
        .zip(&retraced)
        .all(|(then, now)| same(then, now))
    {
        return Err(stale("The element changed since that look"));
    }

    let element = &retraced.last().ok_or_else(unavailable)?.element;
    if element.enabled == Some(false) {
        return Err(Error::new(
            Code::Input,
            "The selected control is disabled; no click was sent",
        )
        .with("next", json!(DISABLED_CONTROL)));
    }
    let frame = element
        .frame
        .as_ref()
        .filter(|frame| drawn(frame))
        .ok_or_else(|| stale("The element is not on screen"))?;
    let (x, y) = click_point(element.role.as_deref(), frame);
    if !Bounds::display(epoch.width, epoch.height).contains(x, y) {
        return Err(stale("The element is off the display"));
    }

    let windows = native::windows::windows()?;
    let expect = element_check(
        element,
        &windows,
        Expect::from_instance(found.instance)?,
        (x, y),
    )
    .map_err(|error| error.with("next", json!(LOOK_AGAIN)))?;
    Ok((x, y, expect))
}

/// Whether what app `pid` has at `(x, y)` is `target`, or lies within it a
/// few elements down. Anything that fails to read is not it.
fn lands_on(pid: i32, (x, y): (f64, f64), target: &AXUIElement) -> bool {
    let Some(app) = ffi::application(pid) else {
        return false;
    };
    let mut scan = Scan::new(Instant::now() + RESOLVE);
    scan.bound(&app);
    let Ok(Some(mut hit)) = ffi::element_at(&app, x as f32, y as f32) else {
        return false;
    };
    for _ in 0..HIT_DEPTH {
        if *hit == *target {
            return true;
        }
        let Some(parent) = scan
            .attribute(Source::Element(&hit), "AXParent")
            .and_then(|parent| parent.downcast::<AXUIElement>().ok())
        else {
            return false;
        };
        hit = parent;
    }
    false
}

/// Whether `now` is the element a look saw as `then`: the same role,
/// subrole, title, description, identifier and menu, and for a static text
/// the same value, each proven by both reads. Any other value may change,
/// as typing changes it, and so may the frame.
fn same(then: &Seen, now: &Retraced) -> bool {
    let (was, is) = (&then.element, &now.element);
    let text = was.role.as_deref() == Some("AXStaticText");
    let proven = |unproven: &[&str]| {
        unproven
            .iter()
            .all(|attribute| !text && *attribute == "AXValue")
    };
    was.role == is.role
        && was.subrole == is.subrole
        && was.title == is.title
        && was.description == is.description
        && was.identifier == is.identifier
        && was.menu == is.menu
        && (!text || was.value == is.value)
        && proven(&then.unproven)
        && proven(&now.unproven)
}

#[cfg(test)]
mod tests;
