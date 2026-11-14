use super::*;

#[tokio::test]
async fn a_copy_that_launchd_did_not_start_as_the_guests_job_never_opens_its_key() {
    let desktop = atlas_core::desktop::Desktop::from(atlas_core::user::User { uid: 502 }).id();
    let folder = tempfile::tempdir().unwrap();

    let refused = open(desktop, folder.path().join("pairing-secret"))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Identity);
    assert!(
        refused.error.contains("not the guest's own Atlas daemon"),
        "{}",
        refused.error
    );
}
