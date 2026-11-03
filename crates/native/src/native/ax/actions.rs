//! Actions done to a referenced element through Accessibility alone. The
//! element is met again along its own path and proven the one its look
//! saw, its window admitted as input would be, and what it supports read
//! first; it is met and proven again just before the one request that acts,
//! its state read then, and what it reads as afterwards read back. Nothing
//! is posted, and nothing is asked to move or take focus. An element already
//! as asked is left as it is, and one whose state cannot be read is never
//! toggled blind.
use super::{
    Status, absent,
    bring::{admitted, check_application},
    ffi,
    references::{Found, Looks, Met, kept, meet},
};
use atlas_core::{
    actions::{ACT, ElementAction, RESOLVE, Verification},
    error::{Code, Error, Result, advice::DISABLED_CONTROL},
    process::Instance,
    screen::elements::{Reference, stale},
    session::InputEpoch,
};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFBoolean, CFNull, CFNumber, CFRetained, CFString, CFType, Type};
use serde_json::json;
use std::time::{Duration, Instant};

/// The longest one read may wait for an answer.
const REQUEST: Duration = Duration::from_millis(250);

/// The least time the action itself is given: with less left, it is not
/// asked at all.
const EFFECT: Duration = Duration::from_millis(150);

/// How many elements up from an option its control may be: a menu item's
/// control is its menu's pop-up, a row's its table.
const OPTION_DEPTH: usize = 4;

/// The most action names an element may list.
const ACTIONS: usize = 64;

/// What an agent does about an element that does not take an action.
const INSTEAD: &str = "Click it with atlas click --ref instead, or run atlas elements again.";

/// What an action on an element came to: whether it reads as asked, the
/// process it acted in, and whether the app took a request to change it
/// (not when it already read as asked).
#[derive(Debug, Clone, Copy)]
pub(in crate::native) struct Acted {
    pub(in crate::native) verification: Verification,
    pub(in crate::native) instance: Instance,
    pub(in crate::native) changed: bool,
}

/// The one request that acts on an element.
enum Effect {
    Perform(&'static str),
    Set(&'static str, CFRetained<CFType>),
}

/// What an element should read as once acted on.
enum Wanted {
    /// This attribute's two-way state, and whether acting while it cannot
    /// be read would toggle it blind.
    State {
        attribute: &'static str,
        on: bool,
        blind: bool,
    },
    Value(CFRetained<CFType>),
    /// Nothing it reports tells, as after a press.
    Unknown,
}

/// Carry out `action` on the element `reference` names, within [`ACT`]: of
/// the app `named` when a batch names one, and in window `window` when it
/// names one. `guard` proves the session, control and permission before
/// each step and just before the request that acts.
///
/// # Errors
///
/// Refuses, with nothing done, a reference [`kept`] or [`meet`] refuses, an
/// element in another window, an option outside its control, a window a
/// sheet or modal blocks, a disabled control, a password field's value, an
/// action the element does not support, a state it cannot read when acting
/// would toggle it, and an app too slow to leave time for the action. An
/// action the app did not acknowledge is uncertain.
pub(in crate::native) fn act_on(
    looks: &Looks,
    reference: Reference,
    action: &ElementAction,
    epoch: &InputEpoch,
    named: Option<Instance>,
    window: Option<i64>,
    guard: &dyn Fn() -> Result<()>,
) -> Result<Acted> {
    let deadline = Instant::now() + ACT;
    guard()?;
    let control = kept(looks, reference, epoch, named)?;
    let option = match action {
        ElementAction::Pick(option) => Some(kept(looks, *option, epoch, Some(control.instance()))?),
        _ => None,
    };

    let (met, picked) = meet_all(&control, option.as_ref(), deadline)?;
    if window.is_some_and(|window| met.expect.window() != Some(window)) {
        return Err(stale("That element is not in the window named"));
    }
    if let Some(picked) = &picked
        && !within(&picked.element.object, &met.element.object, deadline)?
    {
        return Err(Error::new(
            Code::Input,
            "That option is not one of that control's; nothing was sent",
        ));
    }
    admitted(&met.expect, deadline)?;
    let (effect, wanted) = plan(action, &met, picked.as_ref(), deadline)?;

    // Met and proven again just before the request that acts.
    guard()?;
    let (again, picked) = meet_all(&control, option.as_ref(), deadline)?;
    if again.expect != met.expect {
        return Err(stale("The element moved to another window"));
    }
    let object: &AXUIElement = &picked.as_ref().unwrap_or(&again).element.object;
    let instance = control.instance();
    if let Wanted::State {
        attribute,
        on,
        blind,
    } = &wanted
        && !needed(state(object, *attribute, deadline)?, *on, *blind)?
    {
        return Ok(Acted {
            verification: Verification::Matched,
            instance,
            changed: false,
        });
    }

    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining < EFFECT {
        return Err(slow());
    }
    guard()?;
    check_application(Some(instance))
        .map_err(|cause| stale("The app it is in quit or was replaced").with_source(cause))?;
    // A timeout that fails to set leaves the process's own, which only
    // changes how long the app is waited for; its answer is judged alike.
    let _ = ffi::set_timeout(object, remaining);
    let status = match &effect {
        Effect::Perform(name) => ffi::act(object, &CFString::from_static_str(*name)),
        Effect::Set(attribute, value) => {
            ffi::set(object, &CFString::from_static_str(*attribute), value)
        }
    };
    acknowledged(status)?;
    Ok(Acted {
        verification: read_back(object, &wanted, deadline),
        instance,
        changed: true,
    })
}

/// The control `control` names met again, and the option `option` names
/// when there is one, each within what is left before `deadline`.
fn meet_all(
    control: &Found,
    option: Option<&Found>,
    deadline: Instant,
) -> Result<(Met, Option<Met>)> {
    let met = meet(control, left(deadline)?.min(RESOLVE))?;
    let picked = match option {
        Some(option) => Some(meet(option, left(deadline)?.min(RESOLVE))?),
        None => None,
    };
    Ok((met, picked))
}

/// The request that carries out `action` on the element `met` (or for a
/// pick, on the option `picked` of it) and what it should read as once
/// done, from what it says it supports.
fn plan(
    action: &ElementAction,
    met: &Met,
    picked: Option<&Met>,
    deadline: Instant,
) -> Result<(Effect, Wanted)> {
    let element = &met.element.element;
    let object: &AXUIElement = &met.element.object;
    let role = element.role.as_deref();
    if *action != ElementAction::ScrollIntoView && element.enabled == Some(false) {
        return Err(disabled());
    }
    let press = |object: &AXUIElement, what: &str| {
        if supports(object, "AXPress", deadline)? {
            Ok(Effect::Perform("AXPress"))
        } else {
            Err(unsupported(what))
        }
    };

    Ok(match action {
        ElementAction::Press => (press(object, "pressed")?, Wanted::Unknown),
        ElementAction::ScrollIntoView => {
            if !supports(object, "AXScrollToVisible", deadline)? {
                return Err(unsupported("scrolled into view"));
            }
            (Effect::Perform("AXScrollToVisible"), Wanted::Unknown)
        }
        ElementAction::SetValue(text) => {
            if secure(role, element.subrole.as_deref()) {
                return Err(Error::new(
                    Code::Input,
                    "Atlas never sets a password field's value; nothing was sent",
                ));
            }
            if !settable(object, "AXValue", deadline)? {
                return Err(unsupported("given a value"));
            }
            let value = value_for(text, read(object, "AXValue", deadline)?.as_deref())?;
            (Effect::Set("AXValue", value.clone()), Wanted::Value(value))
        }
        ElementAction::Check(on) => {
            let radio = role == Some("AXRadioButton");
            if !radio && role != Some("AXCheckBox") {
                return Err(unsupported("checked or unchecked"));
            }
            if radio && !on {
                return Err(Error::new(
                    Code::Input,
                    "A radio button is unchecked by checking another; nothing was sent",
                ));
            }
            (
                press(object, "checked or unchecked")?,
                Wanted::State {
                    attribute: "AXValue",
                    on: *on,
                    blind: true,
                },
            )
        }
        ElementAction::Expand(on) => {
            // An outline row tells it by AXDisclosing, a pop-up or combo box
            // by AXExpanded, and a disclosure triangle by its value.
            let mut told = None;
            for attribute in ["AXExpanded", "AXDisclosing"] {
                if state(object, attribute, deadline)?.is_some() {
                    told = Some(attribute);
                    break;
                }
            }
            let attribute = match told {
                Some(attribute) => attribute,
                None if role == Some("AXDisclosureTriangle") => "AXValue",
                None => return Err(unsupported("expanded or collapsed")),
            };
            let effect = if attribute != "AXValue" && settable(object, attribute, deadline)? {
                Effect::Set(attribute, flag(*on))
            } else {
                press(object, "expanded or collapsed")?
            };
            let blind = matches!(effect, Effect::Perform(_));
            (
                effect,
                Wanted::State {
                    attribute,
                    on: *on,
                    blind,
                },
            )
        }
        ElementAction::Pick(_) => {
            let picked = picked.ok_or_else(|| stale("That option is no longer kept"))?;
            if picked.element.element.enabled == Some(false) {
                return Err(disabled());
            }
            let object: &AXUIElement = &picked.element.object;
            // Selecting an option twice selects it still, so an option
            // whose selection cannot be read is picked all the same.
            let effect = if settable(object, "AXSelected", deadline)? {
                Effect::Set("AXSelected", flag(true))
            } else {
                press(object, "picked")?
            };
            (
                effect,
                Wanted::State {
                    attribute: "AXSelected",
                    on: true,
                    blind: false,
                },
            )
        }
    })
}

/// Whether an element of `role` and `subrole` holds a password, whose value
/// Atlas never sets.
fn secure(role: Option<&str>, subrole: Option<&str>) -> bool {
    [role, subrole].contains(&Some("AXSecureTextField"))
}

/// The value `text` sets on an element whose value reads as `now`: a number
/// for one that holds a number, text for one that holds text or nothing.
fn value_for(text: &str, now: Option<&CFType>) -> Result<CFRetained<CFType>> {
    match now {
        Some(now) if now.downcast_ref::<CFNumber>().is_some() => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
            .map(|number| CFNumber::new_f64(number).into())
            .ok_or_else(|| {
                Error::new(
                    Code::Input,
                    "The element holds a number: give a number; nothing was sent",
                )
            }),
        Some(now) if now.downcast_ref::<CFString>().is_none() => {
            Err(unsupported("given a value as text or a number"))
        }
        _ => Ok(CFString::from_str(text).into()),
    }
}

/// Whether an element whose state reads as `now` must be acted on to read
/// `on`: not when it already does, and never when it cannot be read and
/// acting would toggle it `blind`.
fn needed(now: Option<bool>, on: bool, blind: bool) -> Result<bool> {
    match now {
        Some(now) => Ok(now != on),
        None if blind => Err(Error::new(
            Code::Input,
            "The element's state could not be read, so it was left as it is",
        )
        .with("next", json!(INSTEAD))),
        None => Ok(true),
    }
}

/// What the app's answer `status` to the request that acts says: done,
/// refused with nothing changed, or unknown.
fn acknowledged(status: AXError) -> Result<()> {
    match status {
        AXError::Success => Ok(()),
        AXError::ActionUnsupported
        | AXError::AttributeUnsupported
        | AXError::IllegalArgument
        | AXError::NotImplemented => Err(Error::new(
            Code::Input,
            "The app refused the action; nothing changed",
        )
        .with("next", json!(INSTEAD))
        .with_source(Status(status))),
        // A request that timed out, or failed any other way, may have been
        // carried out.
        _ => Err(Error::new(
            Code::Uncertain,
            "The app did not acknowledge the action; it may have happened",
        )
        .uncertain()
        .with_source(Status(status))),
    }
}

/// Whether `object` reads as `wanted`, read before `deadline`.
fn read_back(object: &AXUIElement, wanted: &Wanted, deadline: Instant) -> Verification {
    let reads = match wanted {
        Wanted::Unknown => None,
        Wanted::State { attribute, on, .. } => state(object, *attribute, deadline)
            .ok()
            .flatten()
            .map(|now| now == *on),
        Wanted::Value(value) => read(object, "AXValue", deadline)
            .ok()
            .flatten()
            .map(|now| *now == **value),
    };
    match reads {
        Some(true) => Verification::Matched,
        Some(false) => Verification::Mismatch,
        None => Verification::NotChecked,
    }
}

/// Whether `option` lies within `control`, at most [`OPTION_DEPTH`]
/// elements down, as each element's parent says now.
fn within(option: &AXUIElement, control: &AXUIElement, deadline: Instant) -> Result<bool> {
    let mut parent = read(option, "AXParent", deadline)?;
    for _ in 0..OPTION_DEPTH {
        let Some(element) = parent.and_then(|parent| parent.downcast::<AXUIElement>().ok()) else {
            return Ok(false);
        };
        if *element == *control {
            return Ok(true);
        }
        parent = read(&element, "AXParent", deadline)?;
    }
    Ok(false)
}

/// Attribute `attribute` of `object` as a two-way state: a flag, or a
/// number 0 or 1. None when it is absent or anything else, as a mixed
/// checkbox's 2 is.
fn state(object: &AXUIElement, attribute: &'static str, deadline: Instant) -> Result<Option<bool>> {
    Ok(read(object, attribute, deadline)?.and_then(|value| two_way(&value)))
}

/// `value` as a two-way state, when it is one.
fn two_way(value: &CFType) -> Option<bool> {
    if let Some(flag) = value.downcast_ref::<CFBoolean>() {
        return Some(flag.as_bool());
    }
    match value.downcast_ref::<CFNumber>()?.as_i64()? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// The value of `object`'s attribute `attribute`: None when it has none.
fn read(
    object: &AXUIElement,
    attribute: &'static str,
    deadline: Instant,
) -> Result<Option<CFRetained<CFType>>> {
    bound(object, deadline)?;
    match ffi::copy(object, &CFString::from_static_str(attribute)) {
        Ok(value) => Ok(value.filter(|value| value.downcast_ref::<CFNull>().is_none())),
        Err(status) if absent(status) => Ok(None),
        Err(status) => Err(unreadable(status)),
    }
}

/// Whether `object` lists `action` among the actions it takes.
fn supports(object: &AXUIElement, action: &'static str, deadline: Instant) -> Result<bool> {
    bound(object, deadline)?;
    let names = match ffi::actions(object) {
        Ok(Some(names)) => names,
        Ok(None) => return Ok(false),
        Err(status) if absent(status) || status == AXError::ActionUnsupported => {
            return Ok(false);
        }
        Err(status) => return Err(unreadable(status)),
    };
    if names.len() > ACTIONS {
        return Err(unreadable(AXError::Failure));
    }
    let action = CFString::from_static_str(action);
    Ok(names
        .iter()
        .any(|name| name.downcast_ref::<CFString>() == Some(&*action)))
}

/// Whether `object`'s attribute `attribute` takes a new value.
fn settable(object: &AXUIElement, attribute: &'static str, deadline: Instant) -> Result<bool> {
    bound(object, deadline)?;
    match ffi::settable(object, &CFString::from_static_str(attribute)) {
        Ok(settable) => Ok(settable),
        Err(status) if absent(status) => Ok(false),
        Err(status) => Err(unreadable(status)),
    }
}

/// Bound the next request to `object` by what is left before `deadline`,
/// at most [`REQUEST`]: its own timeout, so no other read is affected.
fn bound(object: &AXUIElement, deadline: Instant) -> Result<()> {
    if ffi::set_timeout(object, left(deadline)?.min(REQUEST)) != AXError::Success {
        return Err(unreadable(AXError::Failure));
    }
    Ok(())
}

/// What is left before `deadline`; refused once nothing is.
fn left(deadline: Instant) -> Result<Duration> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(slow())
    } else {
        Ok(left)
    }
}

/// A flag's value as a setter takes it.
fn flag(on: bool) -> CFRetained<CFType> {
    CFBoolean::new(on).retain().into()
}

fn slow() -> Error {
    stale("The app answered too slowly to act on the element; nothing was sent")
}

fn unreadable(status: AXError) -> Error {
    stale("What the element supports could not be read; nothing was sent")
        .with_source(Status(status))
}

fn unsupported(what: &str) -> Error {
    Error::new(
        Code::Input,
        format!("The element cannot be {what}; nothing was sent"),
    )
    .with("next", json!(INSTEAD))
}

fn disabled() -> Error {
    Error::new(Code::Input, "The element is disabled; nothing was sent")
        .with("next", json!(DISABLED_CONTROL))
}

#[cfg(test)]
mod tests;
