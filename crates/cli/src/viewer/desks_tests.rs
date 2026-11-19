//! The service with every desktop the settings list: one viewed, any one's
//! status read, the app's requests naming theirs, its Stop for all, and
//! with one desktop listed, every reply as it always was.
use super::*;
use crate::{
    menubar::Line,
    viewer::{
        MENUBAR_RECORD,
        http::{bind, configured},
        two_desktops::{Two, listing, post, read},
    },
};
use atlas_client::test_support::{VM_ID, client_in};
use atlas_core::test_support::{CONFIGURED_UID, SETTINGS};

/// The JSON pointers at which `one` and `other` differ, into `found`.
fn differing(one: &Value, other: &Value, at: &str, found: &mut Vec<String>) {
    match (one, other) {
        (Value::Object(one), Value::Object(other)) => {
            for key in one.keys().chain(other.keys()) {
                let pointer = format!("{at}/{key}");
                match (one.get(key), other.get(key)) {
                    (Some(one), Some(other)) => differing(one, other, &pointer, found),
                    _ => found.push(pointer),
                }
            }
        }
        (Value::Array(one), Value::Array(other)) if one.len() == other.len() => {
            for (index, (one, other)) in one.iter().zip(other).enumerate() {
                differing(one, other, &format!("{at}/{index}"), found);
            }
        }
        _ if one != other => found.push(at.to_owned()),
        _ => {}
    }
}

/// `value` with what `ticking` points at taken out.
fn without(mut value: Value, ticking: &[String]) -> Value {
    for pointer in ticking {
        if let Some(found) = value.pointer_mut(pointer) {
            *found = Value::Null;
        }
    }
    value
}

#[tokio::test]
async fn with_one_desktop_every_reply_of_the_service_is_unchanged_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    // As before there was more than one desktop: no settings followed.
    let mut before = client_in(&base);
    before.controller = atlas_control::Controller::at(CONFIGURED_UID, &base.join("before"));
    let before = configured(before, "127.0.0.1:0".to_owned(), None);
    // Now: the settings followed, listing one desktop, and the station's
    // folder kept.
    let settings = base.join("u.toml");
    std::fs::write(&settings, SETTINGS).unwrap();
    let mut now = client_in(&base);
    now.controller = atlas_control::Controller::at(CONFIGURED_UID, &base.join("now"));
    let now = configured(now, "127.0.0.1:0".to_owned(), Some(&settings));
    unpoisoned(&now.stand_ins).push(Client::clone(&now.client()));
    *unpoisoned(&now.station_home) = Some(base.join("home"));
    now.follow_settings().await;
    assert!(!now.several());

    let id = before.client().desktop().id().as_str().to_owned();
    let named = format!("status.json?desktop={id}");
    let reads = [
        "",
        "status.json",
        named.as_str(),
        "activity.json",
        "sessions.json",
        "screenshots.json",
        "asks.json",
        "pipe.json",
        "cleanup.json?session=nothing",
        "status.json?desktop=elsewhere",
        "nothing.json",
    ];
    for app in [false, true] {
        for route in reads {
            let (first, once) = read(&before, route, app).await;
            let (_, again) = read(&before, route, app).await;
            let mut ticking = Vec::new();
            differing(&once, &again, "", &mut ticking);
            // Its start, and who uses this Mac's screen, which a look answers
            // only within its own deadline.
            ticking.extend(["/incarnation", "/pid", "/consoleName"].map(str::to_owned));
            let (status, said) = read(&now, route, app).await;
            assert_eq!(status, first, "{route}");
            assert_eq!(
                without(said, &ticking),
                without(once, &ticking),
                "{route} as the app: {app}"
            );
        }
    }
    let changes = [
        ("control/release", json!({"lease": "none"})),
        ("control/renew", json!({"lease": "none"})),
        (
            "control/release",
            json!({"desktop": "elsewhere", "lease": "x"}),
        ),
        ("control/release", json!({"desktop": 7, "lease": "x"})),
        ("control/revoke", json!({})),
        ("control/hold", json!({"on": "yes"})),
        ("control/hold", json!({"on": false, "desktop": "elsewhere"})),
        ("app/viewer", json!({"desktop": "elsewhere"})),
        ("control/hold", json!({"on": true})),
        ("nothing", json!({})),
    ];
    for (route, body) in changes {
        for app in [false, true] {
            let was = post(&before, route, &body, app).await;
            let is = post(&now, route, &body, app).await;
            assert_eq!(is, was, "{route} {body} as the app: {app}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_apps_stop_holds_every_listed_desktop() {
    let two = Two::new().await;

    let (status, stopped) = post(&two.viewer, "control/hold", &json!({"on": true}), true).await;

    assert_eq!(status, 200, "{stopped}");
    assert_eq!(stopped["held"], true);
    let each = stopped["desktops"].as_array().unwrap();
    assert_eq!(each[0]["desktop"], "main");
    assert_eq!(each[0]["reply"]["held"], true);
    assert_eq!(each[1]["desktop"], "work");
    assert_eq!(each[1]["reply"]["held"], true);
    assert_eq!(each[0]["uncertain"], false);
    // The stand-in's guest never confirms a Stop: kept here all the same,
    // and named as the one not certain.
    assert_eq!(each[1]["reply"]["delivered"], false, "{stopped}");
    assert_eq!(each[1]["uncertain"], true);
    assert!(two.main.controller.stop_noted(), "Second Login is held");
    assert!(two.work.controller.stop_noted(), "and the VM");
    assert_eq!(two.keeper.asked_for("stop").len(), 1, "its keeper was told");
    assert!(!each[1].to_string().contains("helper"), "{stopped}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_app_request_naming_no_desktop_is_refused_once_two_are_listed() {
    let two = Two::new().await;
    let main = Two::id(&two.main);

    for route in ["status.json", "activity.json", "sessions.json", "asks.json"] {
        let (status, refused) = read(&two.viewer, route, true).await;
        assert_eq!(status, 400, "{route}: {refused}");
        assert_eq!(refused["code"], "input");
        assert_eq!(
            refused["error"],
            "Name the desktop this is for: Atlas serves more than one"
        );
        let (status, _) = read(&two.viewer, &format!("{route}?desktop={main}"), true).await;
        assert_eq!(status, 200, "{route} named");
    }
    let (status, _) = post(&two.viewer, "control/release", &json!({"lease": "x"}), true).await;
    assert_eq!(status, 400);
    let (status, _) = post(
        &two.viewer,
        "control/release",
        &json!({"lease": "x", "desktop": main}),
        true,
    )
    .await;
    assert_ne!(status, 400);
    // Named in the query, as the app names it on every request.
    let (status, _) = post(
        &two.viewer,
        &format!("control/release?desktop={main}"),
        &json!({"lease": "x"}),
        true,
    )
    .await;
    assert_ne!(status, 400);
    let (status, _) = post(&two.viewer, "control/hold", &json!({"on": false}), true).await;
    assert_eq!(status, 400, "a Resume names its desktop");

    // A reader that is not the app reads the desktop viewed, as before.
    let (status, viewed) = read(&two.viewer, "status.json", false).await;
    assert_eq!(status, 200);
    assert_eq!(viewed["desktopKey"], "user");
    // And the list of desktops is of every one.
    let (status, listed) = read(&two.viewer, "desktops.json", true).await;
    assert_eq!(status, 200, "{listed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn any_listed_desktops_status_is_read_while_another_is_viewed() {
    let two = Two::new().await;
    let work = Two::id(&two.work);

    let (status, read_now) = read(&two.viewer, &format!("status.json?desktop={work}"), true).await;

    assert_eq!(status, 200, "{read_now}");
    assert_eq!(read_now["desktopKey"], format!("vm-{VM_ID}"));
    assert_eq!(read_now["controlActive"], false);
    assert_eq!(
        two.viewer.client().desktop().id().as_str(),
        Two::id(&two.main)
    );
    assert!(!read_now.to_string().contains("helper"), "{read_now}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn viewing_another_desktop_releases_control_on_the_one_viewed_before() {
    let two = Two::new().await;
    let work = Two::id(&two.work);
    two.viewer
        .lease
        .lock()
        .await
        .acquire(&two.main.controller)
        .unwrap();

    let (status, viewed) = post(&two.viewer, "view", &json!({"desktop": work}), true).await;

    assert_eq!(status, 200, "{viewed}");
    assert_eq!(viewed, json!({"ok": true, "desktop": work}));
    assert_eq!(two.viewer.client().desktop().id().as_str(), work);
    assert!(!two.viewer.lease.lock().await.active(), "control ended");
    assert_eq!(
        two.main.controller.unresolved().unwrap().len(),
        1,
        "on Second Login, whose helper did not answer: kept for recovery"
    );
    let (_, status_now) = read(&two.viewer, &format!("status.json?desktop={work}"), true).await;
    assert_eq!(status_now["desktopKey"], format!("vm-{VM_ID}"));

    // Only the app views another desktop, and only one served.
    let main = Two::id(&two.main);
    let (status, _) = post(&two.viewer, "view", &json!({"desktop": main}), false).await;
    assert_eq!(status, 403);
    let (status, _) = post(&two.viewer, "view", &json!({"desktop": "elsewhere"}), true).await;
    assert_eq!(status, 409);
    assert_eq!(two.viewer.client().desktop().id().as_str(), work);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_viewed_desktop_falls_back_to_the_default_once_removed() {
    let two = Two::new().await;
    let work = Two::id(&two.work);
    let (status, _) = post(&two.viewer, "view", &json!({"desktop": work}), true).await;
    assert_eq!(status, 200);

    // A service started again views the one viewed last.
    let (_, again) = bind(two.main.clone(), 0, Some(&two.settings))
        .await
        .unwrap();
    unpoisoned(&again.stand_ins).extend([two.main.clone(), two.work.clone()]);
    *unpoisoned(&again.station_home) = Some(two.home.clone());
    again.adopt_viewed();
    again.follow_settings().await;
    assert_eq!(again.client().desktop().id().as_str(), work);

    let output = two.work.selected.desktop.output_dir().to_path_buf();
    std::fs::write(&two.settings, listing(&output, false)).unwrap();
    two.viewer.follow_settings().await;

    assert_eq!(
        two.viewer.client().desktop().id().as_str(),
        Two::id(&two.main)
    );
    let (_, listed) = read(&two.viewer, "desktops.json", true).await;
    let desktops = listed["desktops"].as_array().unwrap();
    assert_eq!(desktops.len(), 1);
    assert_eq!(desktops[0]["viewed"], true);
    assert!(!two.viewer.several());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn desktops_json_lists_each_desktop_with_its_standing_and_a_vms_facts() {
    let two = Two::new().await;
    let (_, before) = read(&two.viewer, "desktops.json", true).await;
    assert_eq!(
        before["desktops"][1]["standing"],
        Value::Null,
        "none read yet"
    );

    let _ = two.viewer.status_for(true).await;
    two.viewer.keep_others_up().await;
    let (status, listed) = read(&two.viewer, "desktops.json", false).await;

    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["viewed"], Two::id(&two.main));
    let main = &listed["desktops"][0];
    assert_eq!(main["name"], "main");
    assert_eq!(main["kind"], "user");
    assert_eq!(main["key"], "user");
    assert_eq!(main["default"], true);
    assert_eq!(main["viewed"], true);
    assert!(main["standing"].is_string(), "{main}");
    assert!(main.get("facts").is_none());
    let work = &listed["desktops"][1];
    assert_eq!(work["id"], VM_ID);
    assert_eq!(work["name"], "work");
    assert_eq!(work["kind"], "vm");
    assert_eq!(work["key"], format!("vm-{VM_ID}"));
    assert_eq!(work["default"], false);
    assert_eq!(work["viewed"], false);
    assert!(work["standing"].is_string(), "{work}");
    assert!(work["facts"]["can"].is_array(), "{work}");
    assert!(work["facts"].get("macos").is_some(), "{work}");
    assert!(!listed.to_string().contains("helper"), "{listed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_either_desktop_through_a_running_service_views_it_beside_no_control() {
    let two = Two::new().await;
    let (lines, mut received) = tokio::sync::mpsc::unbounded_channel();
    *unpoisoned(&two.viewer.app) = Some(lines);
    let (main, work) = (Two::id(&two.main), Two::id(&two.work));

    for id in [&work, &main] {
        let (status, opened) = post(&two.viewer, "app/viewer", &json!({"desktop": id}), true).await;
        assert_eq!(status, 200, "{opened}");
        assert_eq!(received.recv().await, Some(Line::Viewer));
        assert_eq!(two.viewer.client().desktop().id().as_str(), id.as_str());
    }

    two.viewer
        .lease
        .lock()
        .await
        .acquire(&two.main.controller)
        .unwrap();
    let (status, refused) = post(&two.viewer, "app/viewer", &json!({"desktop": work}), true).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["code"], "busy");
    assert_eq!(
        two.viewer.client().desktop().id().as_str(),
        main,
        "the person keeps control"
    );
    assert!(two.viewer.lease.lock().await.active());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_opens_only_the_desktop_viewed_and_never_switches_it() {
    let two = Two::new().await;
    let (lines, mut received) = tokio::sync::mpsc::unbounded_channel();
    *unpoisoned(&two.viewer.app) = Some(lines);
    let (main, work) = (Two::id(&two.main), Two::id(&two.work));

    for window in [true, false] {
        let (status, refused) = post(
            &two.viewer,
            "app/viewer",
            &json!({"desktop": work, "window": window}),
            false,
        )
        .await;
        assert_eq!(status, 403, "{refused}");
        assert_eq!(refused["code"], "permission");
        assert_eq!(
            refused["error"],
            "Atlas shows main now. Only the person switches it to work, in the Atlas app."
        );
        assert_eq!(two.viewer.client().desktop().id().as_str(), main);
        assert!(two.viewer.chosen().is_none(), "nothing kept");
    }
    assert!(received.try_recv().is_err(), "no window opened");

    let (status, opened) = post(&two.viewer, "app/viewer", &json!({"desktop": main}), false).await;
    assert_eq!(status, 200, "{opened}");
    assert_eq!(received.recv().await, Some(Line::Viewer));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stop_holds_a_desktop_listed_since_the_last_settings_read_and_names_settings_it_cannot_read()
 {
    let two = Two::new().await;
    let output = two.work.selected.desktop.output_dir().to_path_buf();
    std::fs::write(&two.settings, listing(&output, false)).unwrap();
    two.viewer.follow_settings().await;
    assert!(!two.viewer.several());
    std::fs::write(&two.settings, listing(&output, true)).unwrap();

    let (status, stopped) = post(&two.viewer, "control/hold", &json!({"on": true}), true).await;

    assert_eq!(status, 200, "{stopped}");
    let each = stopped["desktops"].as_array().unwrap();
    assert_eq!(each[1]["desktop"], "work", "{stopped}");
    assert_eq!(each[1]["reply"]["held"], true);
    assert!(
        two.work.controller.stop_noted(),
        "the desktop listed since is held"
    );
    assert!(!two.viewer.several(), "nothing served changed");
    assert_eq!(
        two.viewer.client().desktop().id().as_str(),
        Two::id(&two.main)
    );

    std::fs::write(&two.settings, "not settings").unwrap();
    let (status, failed) = post(&two.viewer, "control/hold", &json!({"on": true}), true).await;
    assert_eq!(status, 409, "{failed}");
    let each = failed["details"]["desktops"]
        .as_array()
        .or_else(|| failed["desktops"].as_array())
        .unwrap_or_else(|| panic!("{failed}"));
    assert_eq!(each[0]["desktop"], "main");
    assert_eq!(each[0]["reply"]["held"], true, "what is served is held");
    assert_eq!(each[1]["desktop"], "settings", "{failed}");
    assert_eq!(each[1]["uncertain"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_desktop_finds_the_one_service_by_its_station_wide_record() {
    let two = Two::new().await;
    let record = json!({"url": two.viewer.url, "pid": std::process::id()});
    let station = crate::viewer::station::service_record(&two.home);
    std::fs::create_dir_all(station.parent().unwrap()).unwrap();
    std::fs::write(&station, record.to_string()).unwrap();
    *unpoisoned(&two.viewer.discovery) = Some((station.clone(), record));

    two.viewer.follow_settings().await;

    for client in [&two.main, &two.work] {
        let found = crate::viewer::local::record_in(&two.home, client.records());
        assert_eq!(found, station, "{}", client.selected.name);
    }
    let elsewhere = two.home.join("elsewhere");
    assert_eq!(
        crate::viewer::local::record_in(&two.home, &elsewhere),
        elsewhere.join(MENUBAR_RECORD),
        "a folder it does not serve keeps its own"
    );
}
