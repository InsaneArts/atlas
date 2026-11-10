use super::*;

#[test]
fn null_references_are_never_owned() {
    // SAFETY: the call leaves its slot null, which is never adopted.
    let copied = unsafe { copied::<CFType>(|_| AXError::Success) };
    assert!(copied.unwrap().is_none());
}

#[test]
fn a_failed_copy_returns_its_error() {
    // SAFETY: the call leaves its slot null and fails, so nothing is adopted.
    let copied = unsafe { copied::<CFType>(|_| AXError::CannotComplete) };
    assert_eq!(copied.unwrap_err(), AXError::CannotComplete);
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
