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
            Element, INCARNATIONS, LOOK_AGAIN, Limits, Look, Reference, Snapshot, click_point,
            drawn, element_check, stale,
        },
        geometry::Bounds,
        select::shown_name,
    },
    session::InputEpoch,
};
use objc2_application_services::AXUIElement;
use serde::Serialize;
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
/// was reached from, what it reported, the identifying attributes that
/// read left unproven, and those whose getter failed.
#[derive(Clone)]
struct Seen {
    object: Kept,
    parent: Option<usize>,
    element: Element,
    unproven: Vec<&'static str>,
    refused: Vec<&'static str>,
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
    /// The looks of this service process, whose ID and start time begin
    /// each name, so a name another service gave, even one the same ID was
    /// given before, names none of them.
    fn default() -> Self {
        let incarnation = Instance::current().map_or(0, |me| {
            let [seconds, micros] = me.start;
            let started = seconds.wrapping_mul(1_000_000).wrapping_add(micros);
            u16::try_from(started % u64::from(INCARNATIONS)).unwrap_or_default()
        });
        Self {
            shelf: Mutex::new(Shelf {
                looks: VecDeque::new(),
                next: Some(Look {
                    service: std::process::id(),
                    incarnation,
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
pub(super) struct Found {
    instance: Instance,
    chain: Vec<Seen>,
}

impl Found {
    /// The process the element is in.
    pub(super) fn instance(&self) -> Instance {
        self.instance
    }

    /// Whether the look reached the element through a web page: it, or an
    /// element on its path, is a web area. Its role is proven again with
    /// the rest of its identity when the element is met.
    pub(super) fn in_web_page(&self) -> bool {
        self.chain
            .iter()
            .any(|seen| seen.element.role.as_deref() == Some("AXWebArea"))
    }

    /// How many elements up the path to `inner` this element is, when the
    /// look saw `inner` reached from it.
    pub(super) fn above(&self, inner: &Found) -> Option<usize> {
        let depth = inner
            .chain
            .len()
            .checked_sub(self.chain.len())
            .filter(|depth| *depth > 0)?;
        let at = inner.chain.get(depth)?;
        (inner.instance == self.instance && *at.object.0 == *self.chain.first()?.object.0)
            .then_some(depth)
    }
}

/// A referenced element met again where its look saw it: what it reports
/// now and the object it is now, each element it was reached from as read
/// again (nearest first), and what an action on it must reach.
pub(super) struct Met {
    pub(super) element: Retraced,
    pub(super) above: Vec<Retraced>,
    pub(super) expect: Expect,
}

/// Where a referenced element is clicked and what the click must reach,
/// what it was met as then, and what its look kept of it, to find it again
/// just before the click.
#[derive(Clone)]
pub(in crate::native) struct Resolved {
    pub(in crate::native) x: f64,
    pub(in crate::native) y: f64,
    pub(in crate::native) expect: Expect,
    pub(in crate::native) reached: Reached,
    found: Found,
}

#[cfg(test)]
impl Resolved {
    /// A resolution a stand-in engine hands back: kept of nothing, met as a
    /// button 20 points square around `(x, y)`.
    pub(in crate::native) fn at(x: f64, y: f64, expect: Expect) -> Self {
        Self {
            x,
            y,
            expect,
            reached: Reached::button(x, y),
            found: Found {
                instance: Instance::new(0, [0, 0]),
                chain: Vec::new(),
            },
        }
    }
}

/// The element an action reached as it was met just before, for the mark
/// on the picture taken before it: its frame in display points, its role
/// and the name a listing shows it by. Only what that meeting read.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(in crate::native) struct Reached {
    frame: Bounds,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl Reached {
    /// `element` as it was met, when it has a frame.
    pub(in crate::native) fn of(element: &Element) -> Option<Self> {
        Some(Self {
            frame: element.frame.clone()?,
            role: element.role.clone(),
            name: shown_name(element).map(str::to_owned),
        })
    }

    /// A button named Save, 20 points square around `(x, y)`.
    #[cfg(test)]
    pub(in crate::native) fn button(x: f64, y: f64) -> Self {
        Self {
            frame: Bounds {
                x: x - 10.0,
                y: y - 10.0,
                width: 20.0,
                height: 20.0,
            },
            role: Some("AXButton".to_owned()),
            name: Some("Save".to_owned()),
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

    /// What the ask-before-sending gate reads of the element `reference`
    /// names, as its look saw it in `epoch`: the process it is in, its role
    /// and name, and whether it sat in a web page. None when that look is
    /// gone or was taken elsewhere.
    pub(in crate::native) fn described(
        &self,
        reference: Reference,
        epoch: &InputEpoch,
    ) -> Option<(i64, Option<String>, Option<String>, bool)> {
        let found = self.find(reference, epoch, Instant::now()).ok()?;
        let element = &found.chain.first()?.element;
        let name = element
            .title
            .clone()
            .filter(|title| !title.is_empty())
            .or_else(|| element.description.clone().filter(|text| !text.is_empty()));
        Some((
            i64::from(found.instance.pid),
            element.role.clone(),
            name,
            found.in_web_page(),
        ))
    }

    /// What the look `reference` names holds of its element at `now`,
    /// when that look is still kept and was taken in `epoch`.
    fn find(&self, reference: Reference, epoch: &InputEpoch, now: Instant) -> Result<Found> {
        let mut shelf = self.shelf.lock().map_err(|_| unavailable())?;
        let named = reference.look;
        if shelf.next.is_some_and(|next| {
            (next.service, next.incarnation) != (named.service, named.incarnation)
        }) {
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
            refused: traced.refused,
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
    let found = kept(looks, reference, epoch, named)?;
    let (x, y, expect, reached) = locate(&found, epoch)?;
    Ok(Resolved {
        x,
        y,
        expect,
        reached,
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
    let (x, y, _, _) = locate(&resolved.found, epoch)?;
    if (x, y) != (resolved.x, resolved.y) {
        return Err(stale("The element moved while its app was brought forward"));
    }
    let target = resolved.found.chain.first().ok_or_else(unavailable)?;
    if !lands_on(resolved.found.instance.pid, (x, y), &target.object.0) {
        return Err(stale("Something else is at the element's point now"));
    }
    Ok(())
}

/// What the look `reference` names keeps of its element: the look still
/// kept, taken in `epoch`, and of the app `named` when a batch names one.
///
/// # Errors
///
/// Refuses, with nothing sent, a look no longer kept, of another service,
/// epoch or app, and an index it has no element at.
pub(super) fn kept(
    looks: &Looks,
    reference: Reference,
    epoch: &InputEpoch,
    named: Option<Instance>,
) -> Result<Found> {
    let found = looks.find(reference, epoch, Instant::now())?;
    if named.is_some_and(|named| named != found.instance) {
        return Err(stale("That look is of another app than the one named"));
    }
    Ok(found)
}

/// The element `found` names, met again within `time` where its look saw
/// it, with what an action on it must reach: its own window, or the app
/// alone for a menu's item. Nothing is asked to move or take focus.
///
/// # Errors
///
/// Refuses, with nothing sent, an element [`again`] refuses and one whose
/// window cannot be told.
pub(super) fn meet(found: &Found, time: Duration) -> Result<Met> {
    let mut above = again(found, time)?;
    let element = above.pop().ok_or_else(unavailable)?;
    above.reverse();
    // Only a menu bar title is told by its point, and it always has a
    // frame: no point is one that lies on no menu bar.
    let point = element
        .element
        .frame
        .as_ref()
        .map_or((f64::NAN, f64::NAN), |frame| {
            click_point(element.element.role.as_deref(), frame)
        });
    let windows = native::windows::windows()?;
    let expect = element_check(
        &element.element,
        &windows,
        Expect::from_instance(found.instance)?,
        point,
    )
    .map_err(|error| error.with("next", json!(LOOK_AGAIN)))?;
    Ok(Met {
        element,
        above,
        expect,
    })
}

/// The path to the element `found` names, read again from where a walk
/// starts down to it: its app the same process, and each element on it
/// read within `time` and proven the same as the look saw it.
///
/// # Errors
///
/// Refuses, with nothing sent, an app that quit or was replaced, and an
/// element no longer where the look saw it, read too slowly, changed or not
/// proven the same.
fn again(found: &Found, time: Duration) -> Result<Vec<Retraced>> {
    let replaced = |cause: Error| stale("The app it is in quit or was replaced").with_source(cause);
    check_application(Some(found.instance)).map_err(replaced)?;
    let path: Vec<&AXUIElement> = found
        .chain
        .iter()
        .rev()
        .map(|seen| &*seen.object.0)
        .collect();
    let started = Instant::now();
    let retraced = retrace(found.instance.pid, &path, time)?;
    check_application(Some(found.instance)).map_err(replaced)?;
    // A read that ended late may describe an app that has moved on since.
    if started.elapsed() > time {
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
    Ok(retraced)
}

/// Where the element `found` names is clicked now, what the click must
/// reach and what it was met as: met again within [`RESOLVE`] as [`again`]
/// meets it.
///
/// # Errors
///
/// Refuses, with nothing sent, an element [`again`] refuses, a disabled
/// control, and an element off screen or whose window cannot be told.
fn locate(found: &Found, epoch: &InputEpoch) -> Result<(f64, f64, Expect, Reached)> {
    let met = again(found, RESOLVE)?.pop().ok_or_else(unavailable)?;
    let element = &met.element;
    if element.enabled == Some(false) {
        return Err(Error::new(
            Code::Input,
            "The selected control is disabled; no click was sent",
        )
        .with("next", json!(DISABLED_CONTROL)));
    }
    let reached = Reached::of(element)
        .filter(|reached| drawn(&reached.frame))
        .ok_or_else(|| stale("The element is not on screen"))?;
    let (x, y) = click_point(element.role.as_deref(), &reached.frame);
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
    Ok((x, y, expect, reached))
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
/// the same value, each proven by both reads, or failing alike in both, as
/// AppKit's getter fails for a name it lacks. Any other value may change,
/// as typing changes it, and so may the frame.
fn same(then: &Seen, now: &Retraced) -> bool {
    let (was, is) = (&then.element, &now.element);
    let text = was.role.as_deref() == Some("AXStaticText");
    let proven = |unproven: &[&str]| {
        unproven
            .iter()
            .all(|attribute| !text && *attribute == "AXValue")
    };
    // Only a static text's value tells it from another element.
    let identity = |refused: &[&'static str]| -> Vec<&'static str> {
        refused
            .iter()
            .copied()
            .filter(|attribute| text || *attribute != "AXValue")
            .collect()
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
        && identity(&then.refused) == identity(&now.refused)
}

#[cfg(test)]
mod tests;
