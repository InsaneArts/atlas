//! The looks the input service keeps: each scan of an app with the object
//! every element of it is, for one process instance in one input epoch, so
//! a reference names the object that look saw and nothing else. Looks are
//! bounded in number, size and age; a reference to one no longer kept is
//! refused, never matched again by name or place. The objects stay in this
//! process: replies carry only a look's name.
use super::{
    bring::{check_application, target_application},
    ffi::Kept,
    tree::{Observed, Traced, observe},
};
use crate::native;
use atlas_core::{
    actions::{Expect, RESOLVE},
    error::{Code, Error, Result, advice::DISABLED_CONTROL},
    process::Instance,
    screen::{
        elements::{
            Element, LOOK_AGAIN, LOOK_NAMES, Limits, Look, Reference, Snapshot, click_point,
            drawn, element_check, stale,
        },
        geometry::Bounds,
    },
    session::InputEpoch,
};
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

/// The step between the names of looks given one after another: prime to
/// [`LOOK_NAMES`], so no name comes back before every other has been given.
const STRIDE: u64 = 104_729;

/// One element of a look: the object it is, the index of the element it
/// was reached from, and what it reported.
pub(in crate::native) struct Seen {
    object: Kept,
    parent: Option<usize>,
    element: Element,
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

/// The looks kept, oldest first, and how many names were given.
#[derive(Default)]
struct Shelf {
    looks: VecDeque<Taken>,
    given: u64,
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
    /// Where this service's names start, drawn afresh for each service, so
    /// a name an earlier one gave rarely names anything here.
    base: u64,
}

impl Default for Looks {
    fn default() -> Self {
        use std::hash::{BuildHasher, Hasher};

        Self {
            shelf: Mutex::default(),
            base: std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish(),
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
/// what it reported, and its object followed by the object of each element
/// it was reached from, nearest first.
struct Found {
    instance: Instance,
    element: Element,
    chain: Vec<Kept>,
}

/// Where a referenced element is clicked now, and what the click must reach.
#[derive(Debug, Clone)]
pub(in crate::native) struct Resolved {
    pub(in crate::native) x: f64,
    pub(in crate::native) y: f64,
    pub(in crate::native) expect: Expect,
}

fn unavailable() -> Error {
    Error::new(Code::Native, "The input service's looks are unavailable")
}

impl Looks {
    /// Keep `read`, taken in `epoch`, under a new name, letting the oldest
    /// looks go while more would be kept than the bounds allow. The
    /// snapshot comes back to be listed.
    pub(in crate::native) fn keep(
        &self,
        epoch: InputEpoch,
        read: Read,
        now: Instant,
    ) -> Result<(Look, Snapshot)> {
        let mut shelf = self.shelf.lock().map_err(|_| unavailable())?;
        shelf.expire(now);
        while !shelf.looks.is_empty()
            && (shelf.looks.len() >= KEPT || shelf.size() + read.seen.len() > ELEMENTS)
        {
            shelf.looks.pop_front();
        }

        let name = Look::new(self.base % LOOK_NAMES + (shelf.given % LOOK_NAMES) * STRIDE);
        shelf.given += 1;
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
        shelf.expire(now);
        let look = shelf
            .looks
            .iter()
            .find(|look| look.name == reference.look)
            .ok_or_else(|| stale("That look is no longer kept"))?;
        if look.epoch != *epoch {
            return Err(stale("That look was taken before the helper's display changed"));
        }

        let mut at = reference.index;
        let seen = look
            .seen
            .get(at)
            .ok_or_else(|| stale("That look has no element of that number"))?;
        let mut chain = vec![Kept(seen.object.0.clone())];
        // Each element was reached from one found before it, so every step
        // goes to a smaller index and the walk up ends.
        let mut parent = seen.parent;
        while let Some(above) = parent.filter(|above| *above < at) {
            let seen = look.seen.get(above).ok_or_else(unavailable)?;
            chain.push(Kept(seen.object.0.clone()));
            (at, parent) = (above, seen.parent);
        }

        Ok(Found {
            instance: look.instance,
            element: seen.element.clone(),
            chain,
        })
    }
}

/// Read the elements of the app `expect` names, as `atlas elements` lists
/// them, with the object each one is. Refused when that process is gone,
/// or was replaced while it was read.
///
/// # Errors
///
/// Returns the read's own failures as [`super::elements`] does.
pub(in crate::native) fn read(expect: &Expect) -> Result<Read> {
    let instance = target_application(expect).map_err(|cause| {
        Error::new(Code::Elements, "No app is running with that process ID").with_source(cause)
    })?;
    let Observed { snapshot, objects } = observe(instance.pid, Limits::default(), None)?;
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
        })
        .collect();
    Ok(Read {
        instance,
        snapshot,
        seen,
    })
}

/// Where the element `reference` names is now and what a click there must
/// reach, found by reading its app again until that very object is met,
/// through the same elements the look reached it through. A batch `named`
/// for an app finds references only in that very process.
///
/// # Errors
///
/// Refuses, with nothing sent, a look no longer kept or taken in another
/// epoch, an app that quit or was replaced, an object no longer reached the
/// same way or not met in time, an element whose identity changed, a
/// disabled control, and an element off screen or whose window cannot be
/// told.
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
    let replaced = |cause: Error| stale("The app it is in quit or was replaced").with_source(cause);
    check_application(Some(found.instance)).map_err(replaced)?;

    let Some((object, above)) = found.chain.split_first() else {
        return Err(unavailable());
    };
    let limits = Limits {
        time: RESOLVE,
        ..Limits::default()
    };
    let Observed { snapshot, objects } = observe(found.instance.pid, limits, Some(&*object.0))?;
    check_application(Some(found.instance)).map_err(replaced)?;

    let Some(index) = objects
        .iter()
        .position(|traced| *traced.object == *object.0)
    else {
        return Err(stale(if snapshot.incomplete {
            "The element was not found again in time"
        } else {
            "The element is no longer there"
        }));
    };
    if !reached(&objects, index, above) {
        return Err(stale("The element is no longer where that look saw it"));
    }

    let element = snapshot.elements.get(index).ok_or_else(unavailable)?;
    if !same(&found.element, element) {
        return Err(stale("The element changed since that look"));
    }
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
    let expect = element_check(element, &windows, Expect::from_instance(found.instance)?, (x, y))
        .map_err(|error| error.with("next", json!(LOOK_AGAIN)))?;
    Ok(Resolved { x, y, expect })
}

/// Whether the element at `index` of `objects` was reached through exactly
/// the objects `above` names, nearest first, from where the walk started.
fn reached(objects: &[Traced], index: usize, above: &[Kept]) -> bool {
    let mut parent = objects.get(index).and_then(|traced| traced.parent);
    for kept in above {
        match parent.and_then(|at| objects.get(at)) {
            Some(traced) if *traced.object == *kept.0 => parent = traced.parent,
            _ => return false,
        }
    }
    parent.is_none()
}

/// Whether `now` is the element a look saw as `then`: the same role,
/// subrole, title, description, identifier and menu, each read whole, or
/// cut as it was then. Its value and frame may change: typing and
/// scrolling change them.
fn same(then: &Element, now: &Element) -> bool {
    let named = |names: &[String]| -> Vec<String> {
        names
            .iter()
            .filter(|name| *name != "AXValue")
            .cloned()
            .collect()
    };
    then.role == now.role
        && then.subrole == now.subrole
        && then.title == now.title
        && then.description == now.description
        && then.identifier == now.identifier
        && then.menu == now.menu
        && named(&now.unreadable).is_empty()
        && named(&then.truncated) == named(&now.truncated)
}

#[cfg(test)]
mod tests;
