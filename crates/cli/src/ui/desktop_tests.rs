use super::*;

fn work() -> Value {
    json!({"name": "work", "kind": "vm"})
}

#[test]
fn with_one_desktop_listed_a_reply_is_unchanged_byte_for_byte() {
    let replies = [
        json!({"ok": true, "windows": [{"id": 4, "title": "Notes"}]}),
        json!({"ok": false, "code": "transport", "error": "The helper is not answering"}),
        json!(null),
        json!([1, 2]),
    ];
    for reply in replies {
        let named = naming(&reply, None);
        assert!(matches!(named, Cow::Borrowed(_)));
        assert_eq!(named.to_string(), reply.to_string());
    }
}

#[test]
fn with_two_desktops_listed_a_reply_and_a_failure_name_their_desktop() {
    for reply in [
        json!({"ok": true, "windows": []}),
        json!({"ok": false, "code": "platform", "error": "work is a VM desktop"}),
    ] {
        let named = naming(&reply, Some(&work())).into_owned();
        assert_eq!(named["desktop"], work());
        let mut without = named.clone();
        without.as_object_mut().unwrap().remove("desktop");
        assert_eq!(without, reply, "nothing else changes");
    }
}

#[test]
fn a_reply_that_names_its_desktop_already_keeps_its_own() {
    for reply in [
        json!({"ok": true, "desktop": {"kind": "vm", "name": "other"}}),
        json!({"ok": true, "desktop": "other"}),
    ] {
        let named = naming(&reply, Some(&work()));
        assert!(matches!(named, Cow::Borrowed(_)));
        assert_eq!(named.to_string(), reply.to_string());
    }
}

#[test]
fn the_unprinted_null_and_a_reply_that_is_no_object_name_nothing() {
    for reply in [json!(null), json!("text"), json!([{"ok": true}])] {
        assert_eq!(naming(&reply, Some(&work())).into_owned(), reply);
    }
}

#[test]
fn a_desktop_is_named_by_its_settings_name_and_its_kind() {
    let vm = Desktop::from(atlas_core::vm::Vm {
        id: atlas_core::desktop::DesktopId::parse("0123456789abcdef01234567").unwrap(),
    });
    assert_eq!(of("work", &vm), work());

    let dir = tempfile::tempdir().unwrap();
    let user = atlas_client::test_support::client_in(dir.path()).desktop();
    assert_eq!(
        of("second-login", &user),
        json!({"name": "second-login", "kind": "user"})
    );
}
