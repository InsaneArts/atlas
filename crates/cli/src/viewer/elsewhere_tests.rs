//! The sessions of every desktop served, listed beside the viewed one's
//! from the records Atlas keeps, and their Activity read the same way.
use super::*;
use crate::viewer::{
    two_desktops::{Two, read},
    unpoisoned,
};
use atlas_client::{
    host_session::store,
    test_support::{ScriptedKeeper, VM_ID},
};
use atlas_core::host::{Host, HostSession};

/// A session of `host` called `id` that opened Notes on `client`'s desktop,
/// as its records and activity log keep it there.
fn opened_notes(client: &Client, host: Host, id: &str) -> HostSession {
    let session = HostSession::new(host, id).unwrap();
    store::note(
        client,
        &session,
        store::Kind::Open,
        Some(&atlas_client::doing::Doing::new("open").app(Some("Notes"))),
        &Ok(json!({"ok": true})),
    )
    .unwrap();
    let at = atlas_core::time::timestamp();
    let row = json!({"at": at.to_string(), "operation": "open", "ok": true, "actor": "agent",
        "verb": "open", "app": "Notes", "command": "000000000001", "session": session.key()});
    let log = client
        .output_dir()
        .unwrap()
        .join(atlas_client::activity_log::FILE);
    let mut kept = std::fs::read_to_string(&log).unwrap_or_default();
    kept.push_str(&format!("{row}\n"));
    std::fs::write(&log, kept).unwrap();
    session
}

/// Everything the VM's keeper and Atlas VM were asked so far.
fn asked_of(keeper: &ScriptedKeeper) -> (usize, usize) {
    (unpoisoned(&keeper.asked).len(), keeper.runner.ops().len())
}

/// The live entries of `sessions.json` that name the session `key`.
fn live_named<'a>(listed: &'a Value, key: &str) -> Vec<&'a Value> {
    listed["live"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["key"] == key)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_service_lists_a_session_of_a_desktop_that_isnt_viewed_with_its_desktop() {
    let two = Two::new().await;
    let (main, work) = (Two::id(&two.main), Two::id(&two.work));
    let here = opened_notes(&two.main, Host::Claude, "landing");
    let there = opened_notes(&two.work, Host::Codex, "notes-sync");

    let (status, listed) = read(&two.viewer, &format!("sessions.json?desktop={main}"), true).await;

    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["viewed"], main);
    let live = listed["live"].as_array().unwrap();
    assert_eq!(
        live[0]["key"],
        here.key().as_str(),
        "the viewed desktop's first"
    );
    assert_eq!(
        live[0]["desktop"],
        json!({"id": main, "key": "user", "name": "main", "kind": "user"})
    );
    let elsewhere = live_named(&listed, there.key().as_str());
    assert_eq!(elsewhere.len(), 1, "{listed}");
    assert_eq!(
        elsewhere[0]["desktop"],
        json!({"id": VM_ID, "key": format!("vm-{VM_ID}"), "name": "work", "kind": "vm"})
    );
    assert_eq!(elsewhere[0]["host"], "codex");
    assert!(elsewhere[0]["latest"]["says"].is_string(), "{listed}");
    assert!(elsewhere[0].get("hostSession").is_none(), "{listed}");
    let history = listed["sessions"].as_array().unwrap();
    assert!(
        history
            .iter()
            .all(|past| past["key"] != there.key().as_str()),
        "the history is the viewed desktop's"
    );

    // Its Activity reads from its own records, with no picture of it.
    let route = format!("activity.json?session={}&desktop={work}", there.key());
    let (status, activity) = read(&two.viewer, &route, true).await;
    assert_eq!(status, 200, "{activity}");
    let rows = activity["activity"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{activity}");
    assert_eq!(rows[0]["session"], there.key().as_str());
    for picture in PICTURES {
        assert!(rows[0].get(picture).is_none(), "{picture} in {activity}");
    }
    assert!(activity["sessions"][there.key().as_str()].is_object());
    // And the viewed desktop's still names its pictures.
    let route = format!("activity.json?session={}&desktop={main}", here.key());
    let (_, viewed) = read(&two.viewer, &route, true).await;
    assert!(viewed["activity"][0].get("before").is_some(), "{viewed}");

    assert_eq!(
        two.viewer.client().desktop().id().as_str(),
        main,
        "nothing switched the view"
    );
    assert!(!listed.to_string().contains("helper"), "{listed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_vms_sessions_are_listed_from_records_without_asking_the_vm() {
    let two = Two::new().await;
    let (main, work) = (Two::id(&two.main), Two::id(&two.work));
    let there = opened_notes(&two.work, Host::Codex, "notes-sync");
    two.keeper.runner.set_power("stopped");
    *unpoisoned(&two.keeper.away) = true;
    let before = asked_of(&two.keeper);

    let (status, listed) = read(&two.viewer, &format!("sessions.json?desktop={main}"), true).await;
    let route = format!("activity.json?session={}&desktop={work}", there.key());
    let (read_status, activity) = read(&two.viewer, &route, true).await;

    assert_eq!(status, 200, "{listed}");
    assert_eq!(
        live_named(&listed, there.key().as_str()).len(),
        1,
        "{listed}"
    );
    assert_eq!(read_status, 200, "{activity}");
    assert_eq!(activity["activity"].as_array().unwrap().len(), 1);
    assert_eq!(asked_of(&two.keeper), before, "nothing asked of the VM");
    assert_eq!(two.keeper.runner.power(), "stopped", "nor started it");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn while_a_sign_in_keeps_a_vm_private_its_sessions_are_listed_with_no_picture_and_nothing_asked()
 {
    let two = Two::new().await;
    let (main, work) = (Two::id(&two.main), Two::id(&two.work));
    let there = opened_notes(&two.work, Host::Codex, "notes-sync");
    two.work
        .controller
        .interrupt_for_test(&atlas_control::PrivateHold {
            site: "github.com".to_owned(),
            kind: atlas_control::HoldKind::SignIn,
            browser: None,
        });
    let before = asked_of(&two.keeper);

    let (status, listed) = read(&two.viewer, &format!("sessions.json?desktop={main}"), true).await;
    let route = format!("activity.json?session={}&desktop={work}", there.key());
    let (_, activity) = read(&two.viewer, &route, true).await;

    assert_eq!(status, 200, "{listed}");
    let elsewhere = live_named(&listed, there.key().as_str());
    assert_eq!(elsewhere.len(), 1, "{listed}");
    for picture in PICTURES {
        assert!(
            activity["activity"][0].get(picture).is_none(),
            "{picture} in {activity}"
        );
    }
    assert_eq!(asked_of(&two.keeper), before, "nothing asked of the VM");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_of_the_other_desktops_is_shared_for_a_moment_and_forgotten_once_the_feed_tells()
{
    let two = Two::new().await;
    let main = Two::id(&two.main);
    let route = format!("sessions.json?desktop={main}");
    let first = opened_notes(&two.work, Host::Codex, "first");
    let began = Instant::now();
    let (_, listed) = read(&two.viewer, &route, true).await;
    assert_eq!(live_named(&listed, first.key().as_str()).len(), 1);

    let second = opened_notes(&two.work, Host::Claude, "second");
    let (_, kept) = read(&two.viewer, &route, true).await;
    // A machine too busy to read twice within the moment lists afresh.
    if began.elapsed() < KEPT_FOR {
        assert!(
            live_named(&kept, second.key().as_str()).is_empty(),
            "the listing of a moment ago answers"
        );
    }

    two.viewer.forget_elsewhere();
    let (_, afresh) = read(&two.viewer, &route, true).await;
    assert_eq!(live_named(&afresh, second.key().as_str()).len(), 1);
}
