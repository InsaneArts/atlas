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

fn process(pid: i32, uid: u32, start: u64) -> Info {
    Info {
        instance: atlas_core::process::Instance::new(pid, [start, 0]),
        ppid: 1,
        pgid: pid,
        uid,
        ruid: uid,
        svuid: uid,
        zombie: false,
        stopped: false,
        name: "NotificationCent".into(),
    }
}

fn notification_center() -> Option<std::path::PathBuf> {
    Some(NOTIFICATION_CENTER.into())
}

#[test]
fn only_the_logins_own_notification_center_process_is_proven_whatever_its_windows_are_named() {
    let proven = system_with(
        31082,
        503,
        |pid| Some(process(pid, 503, 7)),
        |_| notification_center(),
    );
    assert_eq!(proven, Some(System::NotificationCenter));

    let another_account = system_with(
        31082,
        503,
        |pid| Some(process(pid, 501, 7)),
        |_| notification_center(),
    );
    assert_eq!(another_account, None, "another account's process");

    let mut borrowed = process(31082, 503, 7);
    borrowed.ruid = 0;
    let setuid = system_with(
        31082,
        503,
        |_| Some(borrowed.clone()),
        |_| notification_center(),
    );
    assert_eq!(setuid, None, "a process another account started");

    let mut exited = process(31082, 503, 7);
    exited.zombie = true;
    let gone = system_with(
        31082,
        503,
        |_| Some(exited.clone()),
        |_| notification_center(),
    );
    assert_eq!(gone, None, "a process that has exited");

    let impostor = system_with(
        31082,
        503,
        |pid| Some(process(pid, 503, 7)),
        |_| Some("/Users/Shared/Notification Center".into()),
    );
    assert_eq!(impostor, None, "another program");

    let unknown = system_with(31082, 503, |pid| Some(process(pid, 503, 7)), |_| None);
    assert_eq!(unknown, None, "a program that could not be read");

    for pid in [0, -1, i64::from(i32::MAX) + 1] {
        assert_eq!(
            system_with(pid, 503, |_| unreachable!(), |_| unreachable!()),
            None
        );
    }
}

#[test]
fn a_process_replaced_while_its_program_is_read_is_not_proven() {
    let starts = std::cell::Cell::new(7);
    let replaced = system_with(
        31082,
        503,
        |pid| {
            let start = starts.get();
            starts.set(start + 1);
            Some(process(pid, 503, start))
        },
        |_| notification_center(),
    );
    assert_eq!(replaced, None);
}

#[test]
fn each_process_is_asked_about_once_and_marks_all_its_windows() {
    let listed = |pid, number| Window {
        number,
        pid,
        owner: Some("Notification Center".into()),
        title: None,
        layer: atlas_core::screen::windows::WIDGET_LAYER,
        alpha: 1.0,
        bounds: Bounds {
            x: 8.0,
            y: 38.0,
            width: 180.0,
            height: 180.0,
        },
        system: Some(System::NotificationCenter),
    };
    let mut windows = vec![listed(31082, 1), listed(900, 2), listed(31082, 3)];
    let mut asked = Vec::new();
    attest(&mut windows, |pid| {
        asked.push(pid);
        (pid == 31082).then_some(System::NotificationCenter)
    });
    assert_eq!(asked, [31082, 900]);
    let marks: Vec<_> = windows.iter().map(|window| window.system).collect();
    assert_eq!(
        marks,
        [
            Some(System::NotificationCenter),
            None,
            Some(System::NotificationCenter)
        ],
        "a name or an earlier mark proves nothing"
    );
}
