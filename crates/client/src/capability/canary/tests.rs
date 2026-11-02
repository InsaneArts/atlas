use super::*;
#[test]
fn launcher_output_must_name_a_canary_directory() {
    assert_eq!(
        launched("123 /private/tmp/atlas-canary.Ab12Cd\n"),
        Some((123, "/private/tmp/atlas-canary.Ab12Cd".into()))
    );

    for output in [
        "123 /private/tmp/atlas-canary.",
        "123 /private/tmp/other.Ab12Cd",
        "123 /private/tmp/atlas-canary.x/nested",
        "123 /private/tmp/atlas-canary.a-b",
        "123 /tmp/atlas-canary.Ab12Cd",
        "1 /private/tmp/atlas-canary.Ab12Cd",
        "x /private/tmp/atlas-canary.Ab12Cd",
        "",
    ] {
        assert_eq!(launched(output), None, "{output:?}");
    }
}

#[test]
fn canary_texts_match_the_fixture_bytes() {
    // tests/live/DesktopFixture.swift writes this exact Open text.
    let fixture = include_str!("../../../../../tests/live/DesktopFixture.swift");
    assert!(fixture.contains(&OPEN_TEXT.replace('\n', "\\n")));
    assert!(SAVE_TEXT.ends_with('\u{e9}') && OPEN_TEXT.ends_with("e\u{301}\n"));
}

#[test]
fn buttons_must_be_unique_and_sheets_are_counted() {
    let observed = PanelScan::from(&json!({"controls": [
        {"role": "AXButton", "title": "Save", "enabled": true},
        {"role": "AXButton", "title": "Cancel", "enabled": true},
        {"role": "AXButton", "title": "Cancel", "enabled": false},
        {"role": "AXSheet"},
    ]}));

    assert!(button(&observed, "Save").unwrap().enabled);
    assert!(button(&observed, "Cancel").is_none());
    assert!(button(&observed, "Open").is_none());
    assert_eq!(sheets(&observed), 1);
}

/// The fixture's files are the helper's: read within a bound, and only
/// as regular files.
#[test]
fn fixture_files_are_read_within_a_bound() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    fs::write(&path, b"{\"ok\":true}").unwrap();

    assert_eq!(read_bounded(&path, 11).unwrap(), b"{\"ok\":true}");
    assert!(read_bounded(&path, 10).is_err());

    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(read_bounded(&link, 1024).is_err());
    assert!(read_bounded(dir.path(), 1024).is_err());

    assert_eq!(fixture_text(&path).as_deref(), Some("{\"ok\":true}"));
    assert_eq!(fixture_text(&dir.path().join("missing")), None);
}
