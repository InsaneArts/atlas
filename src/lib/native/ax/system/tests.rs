use super::*;

#[test]
fn dock_gaps_and_separators_are_not_pressable() {
    let frame = || {
        Some(super::super::Frame {
            x: 1120.0,
            y: 920.0,
            width: 16.0,
            height: 40.0,
        })
    };
    assert!(pressable_at(frame(), true, 1128.0, 940.0));
    assert!(
        !pressable_at(frame(), true, 1138.0, 940.0),
        "gap beside an item"
    );
    assert!(
        !pressable_at(frame(), false, 1128.0, 940.0),
        "separator without AXPress"
    );
    assert!(!pressable_at(None, true, 1138.0, 940.0));
}

#[test]
fn only_launchable_dock_subroles_are_pressable_even_with_axpress() {
    for (subrole, expected) in [
        (Some("AXApplicationDockItem"), true),
        (Some("AXDocumentDockItem"), true),
        (Some("AXFolderDockItem"), true),
        (Some("AXMinimizedWindowDockItem"), true),
        (Some("AXURLDockItem"), true),
        (Some("AXSeparatorDockItem"), false),
        (Some("AXTrashDockItem"), false),
        (Some("AXDockExtraDockItem"), false),
        (Some("AXUnknownDockItem"), false),
        (None, false),
    ] {
        let frame = Some(super::super::Frame {
            x: 1120.0,
            y: 920.0,
            width: 40.0,
            height: 40.0,
        });
        assert_eq!(
            launchable(subrole) && pressable_at(frame, true, 1138.0, 940.0),
            expected,
            "{subrole:?}"
        );
    }
}
