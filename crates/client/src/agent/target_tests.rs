use super::*;
use crate::agent::tests::{app_pid, window};
use serde_json::{Value, json};

#[test]
fn helper_identity_resolves_even_when_the_process_is_unavailable_to_the_controller() {
    let pid = i64::from(i32::MAX);
    let start = [123, 456];
    let starts = BTreeMap::from([(pid, start)]);
    let expect = snapshot_expect(pid, &starts).unwrap();
    assert_eq!(
        serde_json::to_value(&expect).unwrap(),
        json!({"pid":pid,"start":start})
    );
    assert_eq!(expect.app_only().instance(), expect.instance());
}

#[test]
fn a_current_snapshot_without_the_process_identity_refuses_instead_of_adopting_it() {
    for starts in [
        BTreeMap::new(),
        BTreeMap::from([(812, [0, 1])]),
        BTreeMap::from([(812, [123, 1_000_000])]),
    ] {
        let error = snapshot_expect(812, &starts).unwrap_err();
        assert_eq!(error.code, Code::Elements);
        assert!(!error.uncertain);
    }
}

#[test]
fn window_snapshot_identities_are_read_but_never_appear_in_the_public_window_rows() {
    let reply = json!({
        "ok":true,"starts":{"812":[123,456]},"windows":[window(42,812,"Editor",0)]
    });
    let snapshot = atlas_core::screen::windows::from_reply(&reply).unwrap();
    let expect = snapshot_expect(snapshot.windows[0].pid, &snapshot.starts).unwrap();
    assert_eq!(
        serde_json::to_value(expect).unwrap(),
        json!({"pid":812,"start":[123,456]})
    );
    let public = json!({"ok":true,"windows":snapshot.windows});
    assert!(public.get("starts").is_none());
    assert!(public["windows"][0].get("start").is_none());
    assert!(public["windows"][0].get("starts").is_none());
}

/// An `apps` row is found by its shown name or its bundle's file name.
#[test]
fn an_app_row_is_named_by_its_shown_or_file_name() {
    let row = app_row(json!({
        "name": "Éditeur de texte",
        "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit",
    }));
    assert!(app_named(&row, "éditeur de texte"));
    assert!(app_named(&row, "TextEdit"));
    assert!(
        !app_named(&row, "Contents"),
        "a folder on the way is not a name"
    );
    assert!(!app_named(&app_row(json!({"name": "Finder"})), "TextEdit"));

    let nested = app_row(json!({
        "name": "Simulator",
        "executable": "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app/Contents/MacOS/Simulator",
    }));

    assert!(
        !app_named(&nested, "Xcode"),
        "the bundle it lives in is not its name"
    );
}

#[test]
fn apps_resolve_to_exactly_one_process() {
    let name = |name: &str| Target::Name(name.into());
    let windows = [window(1, 7, "Dock", 20), window(2, 8, "Calculator", 0)];
    assert_eq!(app_pid(&windows, &name("calculator")).unwrap(), 8);
    assert!(
        app_pid(&windows, &name("Dock")).is_err(),
        "only document windows"
    );
    assert!(app_pid(&windows, &name("Preview")).is_err());

    let alert = [window(3, 5, "Atlas", 8), window(1, 7, "Dock", 20)];
    assert_eq!(
        app_pid(&alert, &name("Atlas")).unwrap(),
        5,
        "a floating alert"
    );

    let twice = [window(1, 8, "TextEdit", 0), window(2, 9, "TextEdit", 0)];
    let error = app_pid(&twice, &name("TextEdit")).unwrap_err();
    assert!(error.error.contains("--pid"), "{}", error.error);
    assert_eq!(app_pid(&twice, &Target::Pid(9)).unwrap(), 9);
}

#[test]
fn an_app_name_keeps_its_identity_only_when_its_processes_agree() {
    let apps = |mut second: Value| {
        second["pid"] = json!(2);
        crate::client::AppList::decode(json!({"ok": true, "apps": [
            {"pid": 1, "name": "TextEdit", "bundleId": "com.apple.TextEdit"}, second
        ]}))
        .unwrap()
    };
    assert!(
        named_app(
            &apps(json!({"name": "textedit", "bundleId": "com.apple.TextEdit"})),
            "TextEdit"
        )
        .is_some()
    );
    for second in [
        json!({"name": "TextEdit", "bundleId": "other.app"}),
        json!({"name": "TextEdit", "bundleId": null}),
    ] {
        assert!(named_app(&apps(second), "TextEdit").is_none());
    }
    assert!(
        named_app(
            &apps(json!({"name": "Other", "bundleId": "other.app"})),
            "Missing"
        )
        .is_none()
    );
}

fn app_row(mut fields: Value) -> crate::client::AppRow {
    fields["pid"] = json!(1);
    crate::client::AppList::decode(json!({"ok": true, "apps": [fields]}))
        .unwrap()
        .rows()[0]
        .clone()
}

#[test]
fn target_selection_pins_the_listed_process_start() {
    let apps = crate::client::AppList::decode(json!({"ok": true,
        "apps": [{"pid": 812, "name": "Editor", "active": true}],
        "starts": {"812": [123, 456]}}))
    .unwrap();
    let app = named_app(&apps, "editor").unwrap();
    let expect = snapshot_expect(app.pid() as i64, apps.starts().unwrap()).unwrap();
    assert_eq!(expect.pid(), 812);
    assert_eq!(
        expect.instance(),
        Some(atlas_core::process::Instance::new(812, [123, 456]))
    );
}

#[tokio::test]
async fn app_targets_keep_valid_process_identities_and_refuse_absent_or_malformed_ones() {
    use crate::test_support::Helper;
    use atlas_core::test_support::UID;
    let directory = tempfile::tempdir().unwrap();
    let (client, helper) = Helper::client(directory.path(), UID);
    for target in [Target::Pid(812), Target::Name("Editor".into())] {
        for starts in [
            None,
            Some(json!({"812": [123, 456]})),
            Some(Value::Null),
            Some(json!({"812": [123]})),
            Some(json!({})),
            Some(json!({"812": [0, 0]})),
        ] {
            let mut reply = json!({"ok": true, "apps": [
                {"pid": 812, "name": "Editor", "active": true}
            ]});
            if let Some(starts) = &starts {
                reply["starts"] = starts.clone();
            }
            let result = helper
                .serve(
                    async {
                        let verified = client.verify().await?;
                        target_among(&client, &verified, &target, None).await
                    },
                    |_| Some(reply.clone()),
                )
                .await;
            match starts {
                Some(value) if value == json!({"812": [123, 456]}) => {
                    assert_eq!(
                        result.unwrap().instance(),
                        Some(atlas_core::process::Instance::new(812, [123, 456]))
                    );
                }
                Some(value) if value == json!({}) || value == json!({"812": [0, 0]}) => {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, Code::Elements);
                    assert_eq!(
                        error.error,
                        "The app's process changed while it was being resolved; look again"
                    );
                }
                None | Some(_) => {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, Code::Protocol);
                    assert_eq!(
                        error.error,
                        "The helper's process identities were malformed"
                    );
                }
            }
        }
    }
}
