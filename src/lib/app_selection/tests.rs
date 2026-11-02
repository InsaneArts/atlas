use super::*;

fn app_list(mut reply: Value) -> AppList {
    reply["ok"] = json!(true);
    AppList::decode(reply).unwrap()
}

fn app_row(mut row: Value) -> AppRow {
    row["pid"] = json!(1);
    app_list(json!({"apps": [row]})).rows()[0].clone()
}

#[test]
fn apps_lists_apps_and_third_party_menu_bar_apps_unless_all() {
    let reply = json!({"ok": true, "apps": [
        {"name": "TextEdit", "pid": 1, "policy": "regular",
         "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit"},
        {"name": "Opie", "pid": 2, "policy": "accessory",
         "executable": "/Applications/Opie.app/Contents/MacOS/Opie"},
        {"name": "Siri", "pid": 3, "policy": "accessory",
         "executable": "/System/Library/CoreServices/Siri.app/Contents/MacOS/Siri"},
        {"name": "loginwindow", "pid": 4, "policy": "accessory",
         "executable": "/usr/libexec/loginwindow"},
        {"name": "cfprefsd", "pid": 5, "policy": "prohibited",
         "executable": "/usr/sbin/cfprefsd"},
    ]});

    let shown = listed(reply.clone(), false);
    let names: Vec<&str> = shown["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|app| app["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["TextEdit", "Opie"]);
    assert_eq!(shown["more"], 3);
    assert_eq!(shown["ok"], true);

    let every = listed(reply, true);
    assert_eq!(every["apps"].as_array().unwrap().len(), 5);
    assert_eq!(every["more"], 0);
}

#[test]
fn a_process_that_cannot_be_placed_is_left_out_unless_all() {
    let reply = json!({"ok": true, "apps": [
        {"name": "Unnamed agent", "pid": 6, "policy": "accessory"},
        {"name": "No policy", "pid": 7,
         "executable": "/Applications/Tool.app/Contents/MacOS/Tool"},
        {"name": "Odd policy", "pid": 8, "policy": "unknown",
         "executable": "/Applications/Odd.app/Contents/MacOS/Odd"},
    ]});

    let shown = listed(reply.clone(), false);
    assert_eq!(shown["apps"], json!([]));
    assert_eq!(shown["more"], 3);

    assert_eq!(listed(reply, true)["apps"].as_array().unwrap().len(), 3);
    assert_eq!(
        listed(json!({"ok": false, "code": "apps"}), false),
        json!({"ok": false, "code": "apps"}),
        "a failed reply is passed on as it is"
    );
}

use serde_json::json;

#[test]
fn app_name_comparison_folds_case_beyond_ascii_and_matches_whole_names() {
    assert!(same_text("TextEdit", "textedit"));
    assert!(same_text("ÉDITEUR", "éditeur"));
    assert!(!same_text("TextEdit", "Text"));
}

#[test]
fn selection_preserves_validated_process_order() {
    let app = |pid: Value| {
        json!({
            "name": "Notes", "pid": pid, "policy": "regular",
            "executable": "/Applications/Notes.app/Contents/MacOS/Notes",
        })
    };
    let apps = AppList::decode(
        json!({"ok": true, "apps": [app(json!(42)), app(json!(7)), app(json!(i32::MAX))]}),
    )
    .unwrap();

    let processes = selected(&apps, Closing::All, None);
    assert_eq!(
        processes
            .into_iter()
            .map(|process| process.pid)
            .collect::<Vec<_>>(),
        [42, 7, i32::MAX as u64]
    );
}

/// `close --all` quits what sessions leave open, never macOS or Atlas.
#[test]
fn closing_everything_spares_macos_and_atlas() {
    let app = |name: &str, policy: &str, executable: &str| json!({"name": name, "pid": 10, "policy": policy, "executable": executable});
    let apps = json!({"apps": [
        app("Finder", "regular", "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"),
        app("loginwindow", "accessory", "/System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow"),
        app("Siri AI", "accessory", "/System/Applications/Siri AI.app/Contents/MacOS/Siri AI"),
        app("universalaccessd", "prohibited", "/usr/sbin/universalaccessd"),
        app("Atlas", "accessory", "/Applications/Atlas.app/Contents/MacOS/atlas"),
        app("Helper", "prohibited", "/Applications/Other.app/Contents/MacOS/Helper"),
        app("Calendar", "regular", "/System/Applications/Calendar.app/Contents/MacOS/Calendar"),
        app("Safari", "regular", "/Applications/Safari.app/Contents/MacOS/Safari"),
        app("Opie QA", "accessory", "/Users/Shared/Opie QA.app/Contents/MacOS/Opie QA"),
    ]});

    let typed = app_list(apps.clone());
    let names = |closing| {
        selected(&typed, closing, Some("/Applications/Atlas.app"))
            .into_iter()
            .map(|process| process.name)
            .collect::<Vec<_>>()
    };

    assert_eq!(names(Closing::All), ["Calendar", "Safari", "Opie QA"]);
    let open = json!({"apps": [apps["apps"][0], apps["apps"][7], apps["apps"][8]]});
    assert_eq!(
        left_open(&app_list(open)),
        ["Safari", "Opie QA"],
        "a held log-out names what close --all would quit"
    );

    let services = json!({"apps": [
        app("Raycast Web Content", "accessory", "/Users/me/com.apple.WebKit.WebContent"),
        app("Slack Helper", "accessory", "/Applications/Slack.app/Contents/Frameworks/Slack Helper.app/Contents/MacOS/Slack Helper"),
        app("Updater", "accessory", "/Applications/Opie.app/Contents/Frameworks/Sparkle.framework/Versions/B/Updater.app/Contents/MacOS/Updater"),
        app("Nameless", "regular", ""),
    ]});

    assert!(
        selected(&app_list(services), Closing::All, None).is_empty(),
        "helpers, services and apps without a program are not apps left open"
    );

    assert!(app_program("/Applications/Notes.app/Contents/MacOS/Notes"));
    assert!(app_program(
        "/Applications/Xcode.app/Contents/Developer/Applications/Simulator.app/Contents/MacOS/Simulator"
    ));
    assert!(!app_program("Notes.app/Contents/MacOS/Notes"));

    assert!(!protected(
        &app_row(json!({"executable": "/usr/local/Cellar/emacs/Emacs.app/Contents/MacOS/Emacs"})),
        None
    ));
    assert!(protected(
        &app_row(json!({"executable": "/usr/libexec/sharingd"})),
        None
    ));

    assert_eq!(names(Closing::App("Finder")), ["Finder"], "refused later");
    assert_eq!(names(Closing::App("opie qa")), ["Opie QA"], "any case");
    assert!(names(Closing::App("Opie")).is_empty(), "whole names only");
    assert!(selected(&typed, Closing::App("Finder"), None)[0].protected);

    // A development build, outside the app, still spares the installed
    // Atlas by its identifier.
    let installed = json!({"executable": "/Applications/Atlas.app/Contents/MacOS/atlas",
        "bundleId": "com.insanearts.atlas"});
    assert!(protected(&app_row(installed), None));
    let window = json!({"executable": "/Applications/Atlas.app/Contents/Helpers/Atlas.app/x",
        "bundleId": "com.insanearts.atlas.window"});
    assert!(protected(&app_row(window), None));
    let other = json!({"executable": "/Applications/Other.app/x",
        "bundleId": "com.insanearts.atlasish"});
    assert!(!protected(&app_row(other), None));
}
