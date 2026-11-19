use super::*;

/// A Mac that can make a VM: macOS 27.1, Apple silicon, 80 GB free, one VM
/// running.
fn able() -> Seen {
    Seen {
        version: Some("27.1".to_owned()),
        silicon: Some(true),
        hypervisor: Some(true),
        free: Some(80_000_000_000),
        running: 1,
    }
}

#[test]
fn a_mac_without_macos_27_cannot_choose_a_vm() {
    for version in [Some("26.4.1"), Some("15.7"), Some("twenty-seven"), None] {
        let seen = Seen {
            version: version.map(str::to_owned),
            ..able()
        };

        assert_eq!(judged(&seen), Some(Lack::Macos), "{version:?}");
    }
    let later = Seen {
        version: Some("28".to_owned()),
        ..able()
    };
    assert_eq!(judged(&later), None);
    assert_eq!(said(judged(&able())), json!({"available": true}));
    assert_eq!(
        said(Some(Lack::Macos)),
        json!({"available": false, "why": "macos"})
    );
}

#[test]
fn a_vm_needs_apple_silicon_that_runs_vms_then_70_gb_free_then_fewer_than_two_running() {
    let intel = Seen {
        silicon: Some(false),
        free: Some(0),
        ..able()
    };
    let no_hypervisor = Seen {
        hypervisor: None,
        ..able()
    };
    let full = Seen {
        free: Some(69_999_999_999),
        running: 2,
        ..able()
    };
    let unread = Seen {
        free: None,
        ..able()
    };
    let busy = Seen {
        running: 2,
        ..able()
    };

    assert_eq!(judged(&intel), Some(Lack::Chip));
    assert_eq!(judged(&no_hypervisor), Some(Lack::Chip));
    assert_eq!(judged(&full), Some(Lack::Disk));
    assert_eq!(judged(&unread), Some(Lack::Disk));
    assert_eq!(judged(&busy), Some(Lack::Running));
    assert_eq!(
        said(judged(&busy)),
        json!({"available": false, "why": "running"})
    );
}
