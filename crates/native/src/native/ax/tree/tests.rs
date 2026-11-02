use super::*;
use crate::native::ax::batch::{BATCH, Slot};
use atlas_core::screen::select::ElementFilter;
use objc2_core_foundation::{CFBoolean, CFNumber, CFString, Type};

#[test]
fn a_malformed_subrole_withholds_the_value_without_reaching_either_getter() {
    for characters in [0, 5000] {
        let batch = Batch {
            slots: BATCH
                .iter()
                .map(|name| match *name {
                    "AXRole" => Slot::Value(CFString::from_static_str("AXTextField").into()),
                    "AXSubrole" => Slot::Value(CFBoolean::new(true).retain().into()),
                    "AXNumberOfCharacters" => Slot::Value(CFNumber::new_isize(characters).into()),
                    _ => Slot::Empty,
                })
                .collect(),
            stalled: false,
        };
        let visit = Visit::root(ffi::system_wide().unwrap(), None, false);
        let mut scan = Scan::new(Instant::now());
        let element = read(&mut scan, &visit, Some(&batch), 0, &[], &[], 812).unwrap();
        assert!(element.subrole.is_none());
        assert!(element.value.is_none());
        assert!(
            scan.stops.is_empty(),
            "either getter would record the expired deadline"
        );
        assert_eq!(element.unreadable, ["AXValue"]);
    }
}

#[test]
fn a_withheld_static_text_value_remains_a_possible_duplicate_without_a_getter() {
    let batch = Batch {
        slots: BATCH
            .iter()
            .map(|name| match *name {
                "AXRole" => Slot::Value(CFString::from_static_str("AXStaticText").into()),
                "AXSubrole" => Slot::Failed(AXError::Failure),
                _ => Slot::Empty,
            })
            .collect(),
        stalled: false,
    };
    let visit = Visit::root(ffi::system_wide().unwrap(), None, false);
    let mut scan = Scan::new(Instant::now());
    let hidden = read(&mut scan, &visit, Some(&batch), 0, &[], &[], 812).unwrap();
    assert!(hidden.value.is_none());
    assert!(
        scan.stops.is_empty(),
        "a getter would record the expired deadline"
    );
    assert!(!scan.errors.keys().any(|name| name.starts_with("AXValue:")));
    let visible = Element {
        index: 1,
        value: Some("Save".into()),
        unreadable: Vec::new(),
        ..hidden.clone()
    };
    let filter = ElementFilter {
        name: Some("Save".into()),
        ..Default::default()
    };
    assert!(filter.matches(&visible));
    assert!(!filter.matches(&hidden));
    assert_eq!(
        [hidden, visible]
            .iter()
            .filter(|element| filter.might_match(element))
            .count(),
        2
    );
}
