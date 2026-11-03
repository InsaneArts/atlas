use super::*;

#[test]
fn optional_app_fields_retain_their_original_presence() {
    let rows = vec![
        json!({"pid": 1}),
        json!({"pid": 2, "name": null, "bundleId": null, "executable": null}),
        json!({"pid": 3, "name": "", "bundleId": "", "executable": "",
            "active": false, "hidden": true, "policy": "future",
            "launchedAt": {"future": true}, "extension": [null, 17]}),
        json!({"pid": i32::MAX, "name": "Éditeur", "bundleId": "org.editor",
            "executable": "/Applications/Editor.app/Contents/MacOS/Editor",
            "active": true, "hidden": false, "policy": "regular", "launchedAt": null}),
    ];
    let reply = json!({"ok": true, "apps": rows, "elapsedMs": 1.5,
        "extension": {"retained": true}});
    let apps = AppList::decode(reply.clone()).unwrap();
    assert_eq!(
        apps.rows().iter().map(AppRow::pid).collect::<Vec<_>>(),
        [1, 2, 3, i32::MAX as u64]
    );
    assert_eq!(apps.project(), reply);

    for field in [
        "name",
        "bundleId",
        "executable",
        "active",
        "hidden",
        "policy",
    ] {
        let values = match field {
            "active" | "hidden" => vec![json!(false), json!(true)],
            "policy" => vec![json!(""), json!("unknown")],
            _ => vec![Value::Null, json!(""), json!("text")],
        };
        for value in values {
            let mut row = json!({"pid": 1});
            row[field] = value;
            let reply = json!({"ok": true, "apps": [row]});
            assert_eq!(AppList::decode(reply.clone()).unwrap().project(), reply);
        }
    }
}

#[test]
fn malformed_app_rows_never_become_an_empty_running_list() {
    for field in [
        "pid",
        "name",
        "bundleId",
        "executable",
        "active",
        "hidden",
        "policy",
    ] {
        for value in [
            Value::Null,
            json!(true),
            json!(false),
            json!(0),
            json!(-1),
            json!(1.5),
            json!("1"),
            json!([]),
            json!({}),
            json!(u64::MAX),
        ] {
            let accepted = match field {
                "pid" => false,
                "name" | "bundleId" | "executable" => value.is_null() || value.is_string(),
                "active" | "hidden" => value.is_boolean(),
                "policy" => value.is_string(),
                _ => unreachable!(),
            };
            if accepted {
                continue;
            }
            let mut row = json!({"pid": 812});
            row[field] = value;
            let reply = json!({"ok": true, "apps": [{"pid": 1}, row]});
            let error = AppList::decode(reply.clone()).unwrap_err();
            assert_eq!(error.code, Code::Protocol, "{reply}");
            assert!(!error.uncertain);
            assert_eq!(
                error.error,
                "The helper's list of running apps was malformed, so Atlas cannot tell what is running"
            );
        }
    }
    for row in [
        json!({}),
        Value::Null,
        json!(false),
        json!([]),
        json!("app"),
        json!({"pid": 2_147_483_648_u64}),
    ] {
        assert_eq!(
            AppList::decode(json!({"ok": true, "apps": [row]}))
                .unwrap_err()
                .code,
            Code::Protocol
        );
    }
}

#[test]
fn public_apps_omit_starts_while_targets_reject_malformed_starts() {
    for starts in [
        Value::Null,
        json!(false),
        json!("legacy"),
        json!([]),
        json!({"812": null}),
        json!({"812": [123]}),
        json!({"812": [123, -1]}),
        json!({"pid": [123, 456]}),
    ] {
        let reply = json!({"ok": true, "apps": [{"pid": 812}], "starts": starts});
        let apps = AppList::decode(reply).unwrap();
        let error = apps.starts().unwrap_err();
        assert_eq!(error.code, Code::Protocol);
        assert_eq!(
            error.error,
            "The helper's process identities were malformed"
        );
        assert!(!error.uncertain);
        assert_eq!(apps.project(), json!({"ok": true, "apps": [{"pid": 812}]}));
    }
    let absent = AppList::decode(json!({"ok": true, "apps": [{"pid": 812}]})).unwrap();
    assert_eq!(absent.starts().unwrap_err().code, Code::Protocol);
    let current = AppList::decode(json!({"ok": true, "apps": [{"pid": 812}],
        "starts": {"812": [123, 456]}}))
    .unwrap();
    assert_eq!(current.starts().unwrap().get(&812), Some(&[123, 456]));
    // A decoded but invalid start stays present for snapshot identity validation.
    let stale = AppList::decode(json!({"ok": true, "apps": [{"pid": 812}],
        "starts": {"812": [0, 1_000_000]}}))
    .unwrap();
    assert_eq!(stale.starts().unwrap().get(&812), Some(&[0, 1_000_000]));
}

#[test]
fn a_refused_app_list_keeps_its_code() {
    let busy = json!({"ok": false, "code": "busy", "uncertain": false,
        "error": "The helper is serving too many connections"});
    let failed = json!({"ok": false, "code": "apps", "uncertain": false,
        "error": "Running applications could not be listed"});

    for (reply, code) in [(busy, Code::Busy), (failed, Code::Apps)] {
        let error = AppList::decode(reply).unwrap_err();
        assert_eq!((error.code, error.uncertain), (code, false));
    }
}

#[test]
fn only_an_ok_reply_with_an_app_array_is_a_list() {
    let empty = json!({"ok": true, "apps": [], "elapsedMs": 1.5});
    assert_eq!(AppList::decode(empty.clone()).unwrap().project(), empty);
    for reply in [
        json!({"ok": true}),
        json!({"ok": true, "apps": null}),
        json!({"ok": true, "apps": {}}),
        json!({"apps": []}),
    ] {
        let error = AppList::decode(reply.clone()).unwrap_err();
        assert_eq!(
            (error.code, error.uncertain),
            (Code::Protocol, false),
            "{reply}"
        );
    }
}
