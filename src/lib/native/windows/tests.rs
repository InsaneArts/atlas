use super::*;
/// The literal keys read here are the framework's exported constants.
#[test]
#[allow(
    unsafe_code,
    reason = "FFI: the framework's constants are extern statics"
)]
fn window_keys_match_the_framework_constants() {
    use objc2_core_graphics::{
        kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer, kCGWindowName, kCGWindowNumber,
        kCGWindowOwnerName, kCGWindowOwnerPID,
    };

    // SAFETY: reading immutable framework constants in a test.
    let constants = unsafe {
        [
            kCGWindowNumber,
            kCGWindowOwnerPID,
            kCGWindowOwnerName,
            kCGWindowName,
            kCGWindowLayer,
            kCGWindowAlpha,
            kCGWindowBounds,
        ]
    };

    let names = [
        "kCGWindowNumber",
        "kCGWindowOwnerPID",
        "kCGWindowOwnerName",
        "kCGWindowName",
        "kCGWindowLayer",
        "kCGWindowAlpha",
        "kCGWindowBounds",
    ];

    for (constant, name) in constants.into_iter().zip(names) {
        assert_eq!(constant.to_string(), name);
    }
}

/// Window dictionaries are read through the same keys CoreGraphics uses.
#[test]
fn window_entries_read_through_typed_keys() {
    use crate::native::cf::{CFDictionary, CFNumber, CFString, CFType};

    let number = |value: f64| CFNumber::new_f64(value);
    let keys = ["X", "Y", "Width", "Height"].map(CFString::from_static_str);
    let sides = [number(1.0), number(2.0), number(30.0), number(40.0)];
    let bounds = CFDictionary::<CFString, CFNumber>::from_slices(
        &keys.iter().map(|key| &**key).collect::<Vec<_>>(),
        &sides.iter().map(|side| &**side).collect::<Vec<_>>(),
    );

    let names = [
        "kCGWindowNumber",
        "kCGWindowOwnerPID",
        "kCGWindowLayer",
        "kCGWindowOwnerName",
        "kCGWindowBounds",
    ]
    .map(CFString::from_static_str);

    let (seven, pid, layer) = (
        CFNumber::new_i64(7),
        CFNumber::new_i64(99),
        CFNumber::new_i64(0),
    );
    let owner = CFString::from_static_str("Finder");
    let values: [&CFType; 5] = [
        seven.as_ref(),
        pid.as_ref(),
        layer.as_ref(),
        owner.as_ref(),
        bounds.as_opaque().as_ref(),
    ];

    let entry = CFDictionary::<CFString, CFType>::from_slices(
        &names.iter().map(|name| &**name).collect::<Vec<_>>(),
        &values,
    );
    let read = read_window(&entry).unwrap();

    assert_eq!((read.number, read.pid, read.layer), (7, 99, 0));
    assert_eq!(read.owner.as_deref(), Some("Finder"));
    assert_eq!(read.title, None);
    assert_eq!(read.alpha, 1.0);
    assert_eq!(
        read.bounds,
        Bounds {
            x: 1.0,
            y: 2.0,
            width: 30.0,
            height: 40.0
        }
    );
}

#[test]
fn the_current_session_lists_windows() {
    // Any logged-in test session has at least the menu bar or Dock.
    let windows = windows().unwrap();
    assert!(windows.iter().all(|w| w.bounds.width >= 0.0 && w.pid > 0));
}
