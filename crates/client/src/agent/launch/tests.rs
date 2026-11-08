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

#[test]
fn an_app_open_in_another_account_says_so_and_not_that_it_may_be_missing() {
    let said = "_LSOpenURLsWithCompletionHandler() failed for the application \
                /Applications/Tailscale.app with error -10829.\n";
    let finished = Finished::decode(&crate::test_support::exited(1, said), OUTPUT);
    assert_eq!(
        serde_json::to_value(not_opened(&finished, &[])).unwrap(),
        json!({"ok": false, "code": "spawn-failed", "uncertain": false,
            "error": "That app runs in one account at a time, and it is already open in another \
                      account on this Mac.",
            "next": atlas_core::error::advice::OPEN_IN_ANOTHER_ACCOUNT, "opened": false})
    );

    let missing = "Unable to find application named 'with error -10829.'";
    let finished = Finished::decode(&crate::test_support::exited(1, missing), OUTPUT);
    let error = serde_json::to_value(not_opened(&finished, &[])).unwrap();
    assert_eq!(
        error["error"], missing,
        "a name is never taken for the error"
    );
    assert!(error.get("next").is_none());
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

#[tokio::test]
async fn opening_a_store_or_a_file_url_to_one_is_refused_before_control_is_taken_or_the_helper_asked()
 {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let copies = directory.path().join("copies");
    let store = directory.path().join("logins.json");
    std::fs::write(&store, b"{}").unwrap();
    let url = format!("file://{}", directory.path().join(".npm%72c").display());
    let escaped_host = format!(
        "file://%6cocalhost{}",
        directory.path().join(".env").display()
    );
    let numeric_host = format!(
        "file://127.0.0.1{}",
        directory.path().join(".npmrc").display()
    );
    let lone_percent = format!("file://{}", directory.path().join("100%/.env").display());
    for target in [
        store.display().to_string(),
        url,
        escaped_host,
        numeric_host,
        lone_percent,
    ] {
        let error = open_in(&client, None, std::slice::from_ref(&target), &[], &copies)
            .await
            .unwrap_err();
        assert_eq!(error.code, Code::Credentials, "{target}: {error}");
    }
    assert!(!copies.exists(), "old copies were pruned");
    assert!(
        !directory.path().join("state").exists(),
        "control was taken"
    );
}

#[tokio::test]
async fn opening_a_link_named_like_a_store_is_refused_though_it_leads_elsewhere() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let settings = directory.path().join("settings");
    std::fs::write(&settings, b"").unwrap();
    let link = directory.path().join("cookies.sqlite");
    std::os::unix::fs::symlink(&settings, &link).unwrap();
    let error = open_in(
        &client,
        None,
        &[link.display().to_string()],
        &[],
        &directory.path().join("copies"),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::Credentials, "{error}");
}

#[tokio::test]
async fn opening_a_file_reference_url_is_refused_before_the_helper_is_asked() {
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    let reference = "file:///.file/id=6571367.1234".to_owned();
    let error = open_in(
        &client,
        None,
        std::slice::from_ref(&reference),
        &[],
        &directory.path().join("copies"),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, Code::Usage, "{error}");
    assert!(
        !directory.path().join("state").exists(),
        "control was taken"
    );
}

#[tokio::test]
async fn opening_a_path_named_like_a_store_that_does_not_exist_is_refused_before_control_is_taken()
{
    let directory = tempfile::tempdir().unwrap();
    let client = crate::test_support::client(directory.path());
    for missing in [
        directory.path().join("missing/.env"),
        directory.path().join("odd://missing/.env"),
        PathBuf::from(".env-missing-here/.npmrc"),
        PathBuf::from(".npmrc"),
    ] {
        let missing = missing.display().to_string();
        let error = open_in(
            &client,
            None,
            std::slice::from_ref(&missing),
            &[],
            &directory.path().join("copies"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::Credentials, "{missing}: {error}");
    }
    assert!(
        !directory.path().join("state").exists(),
        "control was taken"
    );
}

#[test]
fn every_file_named_and_every_path_that_exists_is_checked_as_given() {
    let targets = ["odd:/.env", "https://example.com", ".npmrc"].map(str::to_owned);
    let wanted = [
        handoff::Target::Path(PathBuf::from("/real/settings")),
        handoff::Target::Name("https://example.com".into()),
        handoff::Target::Name(".npmrc".into()),
    ];
    let given = given_paths(None, &targets, &wanted);
    assert_eq!(
        given,
        [Path::new("odd:/.env"), Path::new(".npmrc")],
        "a URL is no file"
    );
    let app = [handoff::Target::Name("Safari".into())];
    assert!(
        given_paths(Some("Safari"), &[], &app).is_empty(),
        "an app's name is the helper's"
    );
}
