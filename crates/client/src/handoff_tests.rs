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
