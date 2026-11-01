//! The Accessibility and CoreFoundation calls, each behind a
//! safe function: elements are made and their attributes, counts, owners
//! and values copied, and what a successful Copy hands over is owned once.
//! Target preparation also sets typed flags and requests supported actions.
#![allow(
    unsafe_code,
    reason = "FFI: Accessibility uses out-pointers and typed CoreFoundation values"
)]
use objc2_application_services::{
    AXCopyMultipleAttributeOptions, AXError, AXUIElement, AXValue, AXValueType,
};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFIndex, CFRange, CFRetained, CFString, CFType, CGPoint, CGSize,
};
use std::{ptr::NonNull, time::Duration};

// The bindings' constructors panic on a null element; these report it.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C-unwind" {
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> Option<NonNull<AXUIElement>>;
    fn AXUIElementCreateSystemWide() -> Option<NonNull<AXUIElement>>;
}

/// Take over the reference a successful Copy wrote to `slot`, if any.
///
/// # Safety
/// `slot` must hold null or a reference the caller owns (from a Copy).
unsafe fn adopt<T: objc2_core_foundation::Type>(slot: *const T) -> Option<CFRetained<T>> {
    // SAFETY: per contract; ownership of the one reference moves here.
    NonNull::new(slot.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) })
}

/// The element of the application `pid`; None when there is none.
pub(super) fn application(pid: libc::pid_t) -> Option<CFRetained<AXUIElement>> {
    // SAFETY: a plain constructor returning an owned element or null.
    let app = unsafe { AXUIElementCreateApplication(pid) }?;
    // SAFETY: the constructor handed over one owned reference.
    Some(unsafe { CFRetained::from_raw(app) })
}

/// The system-wide element; None when there is none.
pub(super) fn system_wide() -> Option<CFRetained<AXUIElement>> {
    // SAFETY: a plain constructor returning an owned element or null.
    let system = unsafe { AXUIElementCreateSystemWide() }?;
    // SAFETY: the constructor handed over one owned reference.
    Some(unsafe { CFRetained::from_raw(system) })
}

/// Bound every request to `element` by `timeout`; for the system-wide
/// element, every request this process makes.
pub(super) fn set_timeout(element: &AXUIElement, timeout: Duration) -> AXError {
    // SAFETY: a live element; only its messaging timeout is set.
    unsafe { element.set_messaging_timeout(timeout.as_secs_f32()) }
}

/// Whether an attribute accepts a setter, without issuing one.
pub(super) fn settable(element: &AXUIElement, name: &CFString) -> Result<bool, AXError> {
    let mut value = 0u8;
    // SAFETY: a live element and string, and a valid Boolean output slot.
    let status = unsafe { element.is_attribute_settable(name, NonNull::from(&mut value)) };
    if status != AXError::Success {
        return Err(status);
    }
    Ok(value != 0)
}

/// Request a typed boolean attribute change.
pub(super) fn set_flag(element: &AXUIElement, name: &CFString, value: bool) -> AXError {
    // SAFETY: both references are live; the value is a CFBoolean, never an
    // unchecked cast of another CoreFoundation type.
    unsafe { element.set_attribute_value(name, CFBoolean::new(value).as_ref()) }
}

/// Copy supported action names. Each member is checked before use.
pub(super) fn actions(
    element: &AXUIElement,
) -> Result<Option<CFRetained<CFArray<CFType>>>, AXError> {
    let mut array: *const CFArray = std::ptr::null();
    // SAFETY: a live element and a valid output slot for the owned Copy result.
    let status = unsafe { element.copy_action_names(NonNull::from(&mut array)) };
    if status != AXError::Success {
        return Err(status);
    }
    // SAFETY: a successful Copy supplied an owned array or null.
    Ok(unsafe { adopt(array) }.map(|array| {
        // SAFETY: CFArray members are CF objects; callers check each type.
        unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(array) }
    }))
}

/// Request an action already checked for support.
pub(crate) fn act(element: &AXUIElement, name: &CFString) -> AXError {
    // SAFETY: live element and action name; no reference escapes the call.
    unsafe { element.perform_action(name) }
}

/// The value of `element`'s attribute `name`: None when a successful Copy
/// handed over nothing, the error when it failed.
pub(super) fn copy(
    element: &AXUIElement,
    name: &CFString,
) -> Result<Option<CFRetained<CFType>>, AXError> {
    let mut value: *const CFType = std::ptr::null();
    // SAFETY: a valid output slot; on success the Copy hands over one owned
    // reference, adopted below.
    let status = unsafe { element.copy_attribute_value(name, NonNull::from(&mut value)) };
    if status != AXError::Success {
        return Err(status);
    }
    // SAFETY: a successful Copy wrote an owned reference (or null).
    Ok(unsafe { adopt(value) })
}

/// The values of every attribute `names` names, in one request, each slot
/// an element's value, an error value or a null.
pub(super) fn copy_many(
    element: &AXUIElement,
    names: &CFArray<CFString>,
) -> Result<Option<CFRetained<CFArray<CFType>>>, AXError> {
    let mut values: *const CFArray = std::ptr::null();
    // SAFETY: the names are an array of CFStrings as required and the
    // output slot is valid; on success the Copy hands over one owned array,
    // adopted below.
    let status = unsafe {
        element.copy_multiple_attribute_values(
            names.as_opaque(),
            AXCopyMultipleAttributeOptions::empty(),
            NonNull::from(&mut values),
        )
    };
    if status != AXError::Success {
        return Err(status);
    }
    // SAFETY: a successful Copy wrote an owned array (or null).
    Ok(unsafe { adopt(values) }.map(|values| {
        // SAFETY: every element of a CFArray is a CF object, each checked
        // before use.
        unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(values) }
    }))
}

/// At most `wanted` values of `element`'s array attribute `name`, from the
/// first.
pub(super) fn copy_values(
    element: &AXUIElement,
    name: &CFString,
    wanted: CFIndex,
) -> Result<Option<CFRetained<CFArray<CFType>>>, AXError> {
    let mut array: *const CFArray = std::ptr::null();
    // SAFETY: a valid output slot; on success the Copy hands over one owned
    // array, adopted below.
    let status =
        unsafe { element.copy_attribute_values(name, 0, wanted, NonNull::from(&mut array)) };
    if status != AXError::Success {
        return Err(status);
    }
    // SAFETY: a successful Copy wrote an owned array (or null).
    Ok(unsafe { adopt(array) }.map(|array| {
        // SAFETY: every element of a CFArray is a CF object; each is checked
        // before use.
        unsafe { CFRetained::cast_unchecked::<CFArray<CFType>>(array) }
    }))
}

/// How many values `element`'s array attribute `name` holds.
pub(super) fn count(element: &AXUIElement, name: &CFString) -> Result<CFIndex, AXError> {
    let mut count: CFIndex = 0;
    // SAFETY: a valid output slot for the count.
    let status = unsafe { element.attribute_value_count(name, NonNull::from(&mut count)) };
    if status != AXError::Success {
        return Err(status);
    }
    Ok(count)
}

/// The value of `element`'s attribute `name` for `parameter`.
pub(super) fn copy_for(
    element: &AXUIElement,
    name: &CFString,
    parameter: &CFType,
) -> Result<Option<CFRetained<CFType>>, AXError> {
    let mut value: *const CFType = std::ptr::null();
    // SAFETY: a live parameter value and a valid output slot; on success the
    // Copy hands over one owned reference, adopted below.
    let status = unsafe {
        element.copy_parameterized_attribute_value(name, parameter, NonNull::from(&mut value))
    };
    if status != AXError::Success {
        return Err(status);
    }
    // SAFETY: a successful Copy wrote an owned reference (or null).
    Ok(unsafe { adopt(value) })
}

/// The element of `app` at the point `x`, `y`.
pub(super) fn element_at(
    app: &AXUIElement,
    x: f32,
    y: f32,
) -> Result<Option<CFRetained<AXUIElement>>, AXError> {
    let mut hit: *const AXUIElement = std::ptr::null();
    // SAFETY: a valid output slot; on success the Copy hands over one owned
    // element, adopted below.
    let status = unsafe { app.copy_element_at_position(x, y, NonNull::from(&mut hit)) };
    if status != AXError::Success {
        return Err(status);
    }
    // SAFETY: a successful Copy wrote an owned reference (or null).
    Ok(unsafe { adopt(hit) })
}

/// The process that owns `element`.
pub(super) fn owner(element: &AXUIElement) -> Result<libc::pid_t, AXError> {
    let mut pid: libc::pid_t = 0;
    // SAFETY: a valid `pid_t` slot for the element's process.
    let status = unsafe { element.pid(NonNull::from(&mut pid)) };
    if status != AXError::Success {
        return Err(status);
    }
    Ok(pid)
}

/// What kind of value `value` holds.
pub(super) fn kind(value: &AXValue) -> AXValueType {
    // SAFETY: a live AXValue.
    unsafe { value.r#type() }
}

/// The point `value` holds, when it holds one.
pub(super) fn point(value: &AXValue) -> Option<CGPoint> {
    let mut point = CGPoint::ZERO;
    // SAFETY: a point value read into a CGPoint; another kind is refused.
    let read = unsafe { value.value(AXValueType::CGPoint, NonNull::from(&mut point).cast()) };
    read.then_some(point)
}

/// The size `value` holds, when it holds one.
pub(super) fn size(value: &AXValue) -> Option<CGSize> {
    let mut size = CGSize::ZERO;
    // SAFETY: a size value read into a CGSize; another kind is refused.
    let read = unsafe { value.value(AXValueType::CGSize, NonNull::from(&mut size).cast()) };
    read.then_some(size)
}

/// The error `value` holds, when it holds one.
pub(super) fn error(value: &AXValue) -> Option<AXError> {
    let mut code = AXError::Failure;
    // SAFETY: an error value read into the 32-bit code it holds; another
    // kind is refused.
    let read = unsafe { value.value(AXValueType::AXError, NonNull::from(&mut code).cast()) };
    read.then_some(code)
}

/// `range` as a value, to ask for an attribute by range.
pub(super) fn range(range: CFRange) -> Option<CFRetained<AXValue>> {
    // SAFETY: a pointer to a live CFRange, the type the value is made as.
    unsafe { AXValue::new(AXValueType::CFRange, NonNull::from(&range).cast()) }
}

/// Where the composed character holding the UTF-16 unit `index` of `text`
/// starts; None when `index` is not inside it.
pub(super) fn composed_start(text: &CFString, index: CFIndex) -> Option<CFIndex> {
    (0..text.length()).contains(&index).then(|| {
        // SAFETY: the index is inside the string.
        unsafe { text.range_of_composed_characters_at_index(index) }.location
    })
}

/// The part of `text` in `range`; None when the range is not inside it.
pub(super) fn substring(text: &CFString, range: CFRange) -> Option<CFRetained<CFString>> {
    let inside = range.location >= 0
        && range.length >= 0
        && range
            .location
            .checked_add(range.length)
            .is_some_and(|end| end <= text.length());
    // SAFETY: a live string, the default allocator and a range within it.
    inside
        .then(|| unsafe { CFString::with_substring(None, Some(text), range) })
        .flatten()
}

/// Values as an app hands them over, for tests.
#[cfg(test)]
pub(super) mod made {
    use super::*;
    use objc2_core_foundation::Type;

    /// An AX error as a batched read returns it in a slot.
    pub(in crate::native::ax) fn error(code: AXError) -> CFRetained<CFType> {
        // SAFETY: a pointer to a live 32-bit code, the type the value is made as.
        unsafe { AXValue::new(AXValueType::AXError, NonNull::from(&code).cast()) }
            .unwrap()
            .into()
    }

    /// The null a batched read returns for an attribute with no value.
    pub(in crate::native::ax) fn null() -> CFRetained<CFType> {
        // SAFETY: a constant CoreFoundation defines for the life of the process.
        unsafe { objc2_core_foundation::kCFNull }
            .unwrap()
            .retain()
            .into()
    }

    pub(in crate::native::ax) fn point(x: f64, y: f64) -> CFRetained<CFType> {
        let point = CGPoint { x, y };
        // SAFETY: a pointer to a live CGPoint, the type the value is made as.
        unsafe { AXValue::new(AXValueType::CGPoint, NonNull::from(&point).cast()) }
            .unwrap()
            .into()
    }

    pub(in crate::native::ax) fn size(width: f64, height: f64) -> CFRetained<CFType> {
        let size = CGSize { width, height };
        // SAFETY: a pointer to a live CGSize, the type the value is made as.
        unsafe { AXValue::new(AXValueType::CGSize, NonNull::from(&size).cast()) }
            .unwrap()
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_references_are_never_owned() {
        // SAFETY: null is never adopted.
        assert!(unsafe { adopt::<CFType>(std::ptr::null()) }.is_none());
    }

    #[test]
    fn a_text_is_cut_only_inside_it() {
        let text = CFString::from_str("abc");
        assert_eq!(composed_start(&text, 1), Some(1));
        assert_eq!(composed_start(&text, 3), None);
        assert_eq!(composed_start(&text, -1), None);
        let part = |location, length| substring(&text, CFRange { location, length });
        assert_eq!(part(0, 2).unwrap().to_string(), "ab");
        assert!(part(2, 2).is_none());
        assert!(part(-1, 1).is_none());
    }
}
