use super::*;
use atlas_core::desktop::DesktopId;
use serde_json::json;

fn vm(id: &str) -> Owner {
    Owner::Vm(RecordKey::vm(&DesktopId::parse(id).unwrap()))
}

#[test]
fn a_frame_service_is_found_only_by_the_desktop_whose_pictures_it_serves() {
    let (work, other) = (
        vm("0123456789abcdef01234567"),
        vm("76543210fedcba9876543210"),
    );
    let helpers = json!({"helperUID": 502, "framePath": "/private/tmp/f.sock"});
    let works = json!({"desktopKey": "vm-0123456789abcdef01234567",
        "framePath": "/private/tmp/f.sock"});

    assert!(Owner::Helper(502).serves(&helpers));
    assert!(!Owner::Helper(503).serves(&helpers));
    assert!(!work.serves(&helpers), "never Second Login's for a VM");
    assert!(work.serves(&works));
    assert!(!other.serves(&works), "nor another VM's");
    assert!(
        !Owner::Helper(502).serves(&works),
        "a VM's names no helper account"
    );
    assert_eq!(work.said().0, "desktopKey");
    assert_eq!(Owner::Helper(502).said(), ("helperUID", json!(502)));
}
