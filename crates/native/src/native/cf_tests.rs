use super::*;
use objc2_core_foundation::Type;

#[test]
fn records_read_typed_values_and_refuse_other_types() {
    let keys = [
        CFString::from_static_str("flag"),
        CFString::from_static_str("count"),
        CFString::from_static_str("name"),
    ];

    let flag = CFBoolean::new(true);
    let count = CFNumber::new_i64(42);
    let name = CFString::from_static_str("helper");
    let values: [&CFType; 3] = [flag.as_ref(), count.as_ref(), name.as_ref()];
    let dictionary = CFDictionary::<CFString, CFType>::from_slices(
        &keys.iter().map(|key| &**key).collect::<Vec<_>>(),
        &values,
    );
    // SAFETY: built from typed CF values.
    let record = unsafe { record(dictionary.as_opaque().retain()) };

    assert_eq!(boolean(&record, "flag"), Some(true));
    assert_eq!(integer(&record, "count"), Some(42));
    assert_eq!(float(&record, "count"), Some(42.0));
    assert_eq!(text(&record, "name").as_deref(), Some("helper"));
    assert_eq!(boolean(&record, "count"), None, "wrong type");
    assert_eq!(integer(&record, "missing"), None);
}
