//! Checks of how an array attribute's reply becomes the elements a scan
//! walks, and when it is whole.
use super::{elements, ffi};
use objc2_application_services::AXUIElement;
use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType};

fn value(element: CFRetained<AXUIElement>) -> CFRetained<CFType> {
    element.into()
}

/// Two distinct elements, as an app might list them as children.
fn two_children() -> [CFRetained<CFType>; 2] {
    let pid = libc::pid_t::try_from(std::process::id()).unwrap();
    [
        value(ffi::system_wide().unwrap()),
        value(ffi::application(pid).unwrap()),
    ]
}

#[test]
fn a_short_or_mistyped_child_array_is_not_whole_and_keeps_what_was_read() {
    let two = CFArray::from_retained_objects(&two_children());
    let (read, whole) = elements(&two, 3);
    assert_eq!(read.len(), 2, "what was read is kept");
    assert!(!whole, "a third child was never read");

    let [first, second] = two_children();
    let button: CFRetained<CFType> = CFString::from_str("AXButton").into();
    let mistyped = CFArray::from_retained_objects(&[first, button, second]);
    let (read, whole) = elements(&mistyped, 3);
    assert_eq!(read.len(), 2, "the readable children survive");
    assert!(!whole, "a member that is no element was never read");
}

/// AppKit answers a plain failure for a name it lacks, as TextEdit's text
/// view does for its description: that answer is kept as what was read,
/// while a read that could not complete leaves the attribute unproven.
#[test]
fn a_failed_name_getter_is_an_answer_and_an_unfinished_read_is_unproven() {
    use super::Scan;
    use objc2_application_services::AXError;
    use std::time::{Duration, Instant};

    let mut scan = Scan::new(Instant::now() + Duration::from_secs(1));
    scan.failed(AXError::Failure, "AXDescription");
    assert!(scan.unproven.is_empty());
    assert_eq!(scan.refused, ["AXDescription"]);

    let mut scan = Scan::new(Instant::now() + Duration::from_secs(1));
    scan.failed(AXError::CannotComplete, "AXDescription");
    scan.failed(AXError::Failure, "AXSubrole");
    assert_eq!(scan.unproven, ["AXDescription", "AXSubrole"]);
    assert!(scan.refused.is_empty());
}

#[test]
fn a_child_array_of_exactly_what_was_asked_is_whole() {
    let two = CFArray::from_retained_objects(&two_children());
    let (read, whole) = elements(&two, 2);
    assert_eq!(read.len(), 2);
    assert!(whole);

    let (read, whole) = elements(&two, 1);
    assert_eq!(read.len(), 1, "never more than was asked for");
    assert!(!whole, "a reply longer than asked is not what was asked");
}
