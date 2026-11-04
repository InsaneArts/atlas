use super::*;
use crate::handoff::Copied;
use atlas_core::files::Token;
use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

#[test]
fn a_failed_launch_keeps_copied_paths_or_explicit_null_for_non_unicode_paths() {
    let finished = Finished::decode(&json!({"timedOut": true}), OUTPUT);
    for invalid in [None, Some("from"), Some("to")] {
        let mut copied = vec![Copied {
            from: PathBuf::from("/original/note.txt"),
            to: PathBuf::from("/copies/note.txt"),
            token: Token::new(),
        }];
        let expected = match invalid {
            None => json!([{"from": "/original/note.txt", "to": "/copies/note.txt"}]),
            Some(field) => {
                let path = PathBuf::from(OsString::from_vec(b"/note-\xff.txt".to_vec()));
                if field == "from" {
                    copied[0].from = path;
                } else {
                    copied[0].to = path;
                }
                Value::Null
            }
        };
        let error = not_opened(&finished, &copied);
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"ok": false, "code": "wait-failed", "uncertain": false,
                "error": "macOS did not finish opening that in time; it may still be launching: \
                              run atlas wait --app NAME, or look with atlas apps",
                "timedOut": true, "opened": false, "copied": expected})
        );
    }
    assert!(
        serde_json::to_value(not_opened(&finished, &[]))
            .unwrap()
            .get("copied")
            .is_none()
    );
}

/// Web pages for a browser already running open in a window of their own,
/// so they never land as a tab in another agent's window; anything else
/// opens as it always did.
#[tokio::test]
async fn web_pages_for_a_running_browser_open_in_a_window_of_their_own() {
    use crate::test_support::{Helper, completed};
    use atlas_core::test_support::UID;

    let chrome = json!([{"pid": 7, "name": "Google Chrome", "bundleId": "com.google.Chrome"}]);
    let url = "https://example.com/".to_owned();
    let cases = [
        (
            chrome.clone(),
            vec![url.clone()],
            json!([
                "/usr/bin/open",
                "-n",
                "-a",
                "Google Chrome",
                "--args",
                "--new-window",
                url
            ]),
        ),
        (
            json!([]),
            vec![url.clone()],
            json!(["/usr/bin/open", "-a", "Google Chrome", "--", url]),
        ),
        (
            chrome,
            vec!["notes.txt".to_owned()],
            json!(["/usr/bin/open", "-a", "Google Chrome", "--", "notes.txt"]),
        ),
    ];
    for (apps, targets, expected) in cases {
        let directory = tempfile::tempdir().unwrap();
        let (client, helper) = Helper::client(directory.path(), UID);
        let copies = directory.path().join("copies");
        std::fs::create_dir(&copies).unwrap();
        let mut launched: Vec<Value> = Vec::new();

        helper
            .serve(
                open_in(&client, Some("Google Chrome"), &targets, &[], &copies),
                |request| {
                    if request["op"] == "apps" {
                        return Some(json!({"ok": true, "apps": apps}));
                    }
                    if request["argv"][0] == "/usr/bin/open" {
                        launched.push(request["argv"].clone());
                    }
                    Some(completed(""))
                },
            )
            .await
            .unwrap();

        assert_eq!(launched, [expected], "{apps} {targets:?}");
    }
}

#[test]
fn a_running_app_is_found_by_its_name_or_by_its_bundles_path() {
    let listed = AppList::decode(json!({"ok": true, "apps": [
        {"pid": 7, "name": "Google Chrome", "bundleId": "com.google.Chrome",
         "executable": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"},
        {"pid": 8, "name": "Google Chrome Helper", "bundleId": "com.google.Chrome.helper",
         "executable": "/Applications/Google Chrome.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper"},
    ]}))
    .unwrap();
    let bundle = |app: &str| running_app(&listed, app).and_then(AppRow::bundle_id);

    assert_eq!(bundle("google chrome"), Some("com.google.Chrome"));
    assert_eq!(
        bundle("/Applications/Google Chrome.app"),
        Some("com.google.Chrome")
    );
    assert_eq!(bundle("/Applications/Google Chrome Canary.app"), None);
    assert_eq!(
        bundle("/Applications/Google"),
        None,
        "a path it only begins with"
    );
}
