use super::*;

fn entry(processes: Value) -> Value {
    json!({"copy": "0123456789abcdef0123456789abcdef0123456789abcdef", "state": "placed",
        "node": "1:7", "processes": processes,
        "path": "/Users/guest/Atlas/open/0123456789abcdef0123456789abcdef0123456789abcdef/Probe.app",
        "evidence": {"change": "0123456789abcdef0123456789abcdef0123456789abcdef"}})
}

#[test]
fn a_guests_listing_is_read_with_what_runs_from_each_copy_and_nothing_else_is_guessed() {
    let copy = GuestCopy::of(&entry(json!([]))).unwrap();

    assert_eq!(copy.state, "placed");
    assert_eq!(copy.node.as_deref(), Some("1:7"));
    assert_eq!(copy.processes, Some(Vec::new()));
    assert_eq!(
        copy.folder().as_deref(),
        Some("/Users/guest/Atlas/open/0123456789abcdef0123456789abcdef0123456789abcdef")
    );
    assert_eq!(copy.app().as_deref(), Some("Probe"));
    assert_eq!(copy.name().as_deref(), Some("Probe"));
    assert!(!copy.in_use(&[]));

    let unknown = GuestCopy::of(&entry(json!("unknown"))).unwrap();
    assert_eq!(unknown.processes, None);
    assert!(unknown.in_use(&[]), "what runs from it cannot be told");
    assert!(GuestCopy::of(&entry(json!([4321]))).unwrap().in_use(&[]));
    assert!(
        GuestCopy::of(&entry(json!(["4321"]))).is_none(),
        "malformed"
    );
    assert!(GuestCopy::of(&json!({"copy": "not a token", "state": "placed"})).is_none());
}

#[test]
fn a_guests_copy_an_app_runs_from_is_in_use_and_a_file_holds_no_app() {
    let copy = GuestCopy::of(&entry(json!([]))).unwrap();
    let folder = copy.folder().unwrap();
    let running = crate::client::AppList::decode(json!({"ok": true, "apps": [
        {"pid": 42, "name": "Probe", "executable": format!("{folder}/Probe.app/Contents/MacOS/Probe")}
    ]}))
    .unwrap();

    assert!(copy.in_use(running.rows()));

    let mut file = entry(json!([]));
    file["path"] =
        json!("/Users/guest/Atlas/open/0123456789abcdef0123456789abcdef0123456789abcdef/notes.txt");
    let file = GuestCopy::of(&file).unwrap();
    assert_eq!(file.app(), None);
    assert_eq!(file.name().as_deref(), Some("notes"));
}
