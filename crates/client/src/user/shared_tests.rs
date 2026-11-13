use super::*;

#[test]
fn an_app_is_installed_when_its_bundle_is_in_the_folder() {
    let applications = tempfile::tempdir().unwrap();
    assert!(!installed_in(applications.path(), "Firefox"));
    std::fs::create_dir(applications.path().join("Firefox.app")).unwrap();
    assert!(installed_in(applications.path(), "Firefox"));
    assert!(!installed_in(applications.path(), "Google Chrome"));
}
