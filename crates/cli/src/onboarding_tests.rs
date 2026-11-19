use super::*;
use atlas_client::test_support::VM_ID;
use atlas_core::test_support::CONFIGURED_UID;

/// Settings listing the Second Login `main` and the VM `work`, the one
/// named `default` the default.
fn settings(directory: &Path, default: &str) -> std::path::PathBuf {
    let path = directory.join("atlas.toml");
    let text = format!(
        "default = \"{default}\"\noutput_dir = \"{}\"\n\n[[desktops]]\nname = \"main\"\nkind = \
         \"user\"\nhelper_uid = {CONFIGURED_UID}\nsocket_path = \"c.sock\"\nidentity_probe = \
         \"probe\"\ninput_socket = \"i.sock\"\n\n[[desktops]]\nname = \"work\"\nkind = \
         \"vm\"\nid = \"{VM_ID}\"\n",
        directory.join("output").display()
    );
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn copying_sign_ins_and_home_privacy_are_refused_on_a_vm() {
    let directory = tempfile::tempdir().unwrap();

    let refused = second_login_only(&settings(directory.path(), "work")).unwrap_err();

    assert_eq!(refused.code, Code::Platform);
    assert_eq!(
        refused.error,
        "Copying your sign-ins works only with Second Login."
    );
    second_login_only(&settings(directory.path(), "main")).unwrap();
}
