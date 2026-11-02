use super::*;

#[test]
fn hello_describes_this_process_in_its_session() {
    let reply = hello(&manager_name(), None);
    assert_eq!(reply["user"]["uid"], nix::unistd::getuid().as_raw());
    assert!(reply["user"]["name"].is_string());
    assert!(
        reply["user"]["home"]
            .as_str()
            .is_some_and(|home| home.starts_with('/'))
    );
    assert_eq!(reply["daemon"]["pid"], std::process::id());
    assert_eq!(reply["daemon"]["protocol"], 1);
    assert!(crate::session::has_capability(
        &reply["daemon"],
        crate::protocol::SESSION_PIN
    ));
    assert!(reply["permissions"]["screenCapture"].is_boolean());
    assert!(
        reply["display"]["scale"]
            .as_f64()
            .is_some_and(|scale| scale >= 1.0)
    );
}
