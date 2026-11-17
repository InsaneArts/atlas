use super::*;

#[test]
fn a_file_url_names_its_path_on_this_mac_with_escapes_decoded() {
    assert_eq!(
        local_file("file:///tmp/a%20b/.env"),
        Some(PathBuf::from("/tmp/a b/.env"))
    );
    assert_eq!(
        local_file("FILE://localhost/tmp/x?y#z"),
        Some(PathBuf::from("/tmp/x"))
    );
    assert_eq!(local_file("file:/tmp/x"), Some(PathBuf::from("/tmp/x")));
    // macOS opens a file URL's path on this Mac whatever host it names.
    assert_eq!(
        local_file("file://other-mac/tmp/x"),
        Some(PathBuf::from("/tmp/x"))
    );
    assert_eq!(
        local_file("file://127.0.0.1/tmp/.env"),
        Some(PathBuf::from("/tmp/.env"))
    );
    assert_eq!(
        local_file("file://%6cocalhost/tmp/.env"),
        Some(PathBuf::from("/tmp/.env")),
        "an escaped host"
    );
    assert_eq!(local_file("https://example.com/.env"), None);
    assert_eq!(
        local_file("file:///tmp/100%/.env"),
        Some(PathBuf::from("/tmp/100%/.env")),
        "a lone % stays, as macOS reads it"
    );
    assert_eq!(local_file("file://host-without-path"), None);
}

#[test]
fn a_file_url_naming_no_file_is_refused_rather_than_handed_on() {
    let error = refuse_targets(
        &[Target::Name("file://host-without-path".into())],
        &Copies::in_shared_folder(),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::Usage);
    assert!(
        refuse_targets(
            &[Target::Name("https://example.com".into())],
            &Copies::in_shared_folder()
        )
        .is_ok()
    );
}

#[test]
fn a_file_url_with_an_escaped_slash_in_a_name_is_refused_rather_than_judged_as_another_file() {
    assert_eq!(
        local_file("file:///Users/person/.ssh/key%2F..%2Fconfig"),
        None
    );
    let error = refuse_targets(
        &[Target::Name("file:///tmp/a%2Fb".into())],
        &Copies::in_shared_folder(),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::Usage);
}

#[test]
fn only_a_scheme_at_the_start_makes_a_url() {
    assert!(has_scheme("https://example.com"));
    assert!(has_scheme("file:///tmp/x"));
    assert!(!has_scheme("/tmp/odd://missing/.env"));
    assert!(!has_scheme("dir/notes.txt"));
    assert!(!has_scheme("2x:thing"));
}

#[test]
fn a_vms_copies_are_its_guests_and_its_own_records_folder_names_them() {
    let vm = DesktopId::parse(crate::test_support::VM_ID).unwrap();
    let desktop = Desktop::from(atlas_core::vm::Vm { id: vm.clone() });
    let output = Path::new("/tmp/atlas-output");

    assert!(Copies::of(&desktop).unwrap().in_a_guest());
    let records = output.join("desktops").join(RecordKey::vm(&vm).as_str());
    assert!(Copies::kept_for(&records).in_a_guest());
    assert!(!Copies::kept_for(output).in_a_guest(), "Second Login's");
    assert!(
        !Copies::kept_for(&output.join("desktops").join("user")).in_a_guest(),
        "no VM's"
    );
    assert!(
        !Copies::kept_for(&output.join("elsewhere").join(RecordKey::vm(&vm).as_str())).in_a_guest(),
        "not beneath desktops"
    );
}

#[test]
fn a_guests_copy_is_known_here_only_by_its_inventory() {
    let vm = DesktopId::parse(crate::test_support::VM_ID).unwrap();
    let copies = Copies::of(&Desktop::from(atlas_core::vm::Vm { id: vm })).unwrap();
    let (placed, staging) = (Token::new(), Token::new());
    let root = "/Users/guest/Library/Application Support/Atlas/open";
    let session = crate::host_session::Key::of(crate::host_session::Host::Codex, "one");

    assert!(copies.exists(&placed), "not listed: counts as there");
    assert_eq!(copies.identity(&placed, &session), None, "nothing proven");
    assert_eq!(copies.app(&placed), None);
    assert!(copies.location(&placed).ends_with(placed.as_str()));
    assert!(!copies.holds(Path::new(root)), "no path here is a guest's");
    assert!(copies.prune(&[]).is_empty(), "a guest's age is not known");
    assert_eq!(
        copies.token_of(&format!("{root}/{placed}/Probe.app")),
        Some(placed.clone())
    );
    assert_eq!(
        copies.token_of(&format!("{root}/{placed}")),
        Some(placed.clone())
    );
    assert_eq!(copies.token_of(&format!("{root}/notes.txt")), None);

    let records = tempfile::tempdir().unwrap();
    let owners = crate::vm::copies::Owners::in_records(records.path());
    for copy in [&placed, &staging] {
        owners
            .keep(&crate::vm::copies::Owner {
                copy: copy.clone(),
                session: Some(session.clone()),
                task: None,
            })
            .unwrap();
    }
    let listed = copies.listed(
        vec![
            crate::vm::copies::GuestCopy {
                copy: placed.clone(),
                state: "placed".into(),
                node: Some("1:100".into()),
                processes: Some(Vec::new()),
                path: Some(format!("{root}/{placed}/Probe.app")),
            },
            crate::vm::copies::GuestCopy {
                copy: staging.clone(),
                state: "staging".into(),
                node: Some("1:101".into()),
                processes: Some(Vec::new()),
                path: None,
            },
        ],
        records.path(),
    );
    assert!(listed.exists(&placed));
    assert!(!listed.exists(&Token::new()), "listed and absent: gone");
    assert_eq!(
        listed.identity(&placed, &session),
        Some(CopyIdentity {
            folder: format!("{root}/{placed}"),
            node: "1:100".into(),
        })
    );
    assert_eq!(listed.identity(&staging, &session), None, "not placed yet");
    let another = crate::host_session::Key::of(crate::host_session::Host::Codex, "two");
    assert_eq!(
        listed.identity(&placed, &another),
        None,
        "another session's"
    );
    owners.forget(&placed).unwrap();
    assert_eq!(
        listed.identity(&placed, &session),
        None,
        "no record: nobody's"
    );
    assert_eq!(listed.app(&placed).as_deref(), Some("Probe"));
    assert_eq!(listed.location(&placed), format!("{root}/{placed}"));
}
