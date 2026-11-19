use super::*;
use atlas_session::compatibility::{CHANGES, MACHINE, MIRRORS, STAGE, UPDATE_BOUND};

/// A guest of `digest` and `version`, declaring every capability a machine
/// guest declares, beside this Mac at build `ours` of version 0.2.0.
fn facts(digest: Option<&str>, version: Option<&str>, ours: Option<&str>) -> Facts {
    Facts {
        declared: MACHINE.iter().map(|name| (*name).to_owned()).collect(),
        guest: Build::new(digest, version),
        own: Build::new(ours, Some("0.2.0")),
    }
}

#[test]
fn every_capability_the_update_and_mirror_paths_check_is_named_in_machine() {
    for checked in [STAGE, UPDATES, UPDATE_BOUND, CHANGES, MIRRORS] {
        assert!(MACHINE.contains(&checked), "{checked}");
    }
}

#[test]
fn a_guest_of_this_macs_very_build_is_current_whatever_its_version_says() {
    let same = facts(Some("b1"), Some("9.0.0"), Some("b1"));

    assert_eq!(same.verdict(), Verdict::Current);
    assert_eq!(advice("work", &same), "Run atlas doctor --desktop work.");
    assert_eq!(said("work", &same), json!({"state": "current"}));
}

#[test]
fn an_older_version_or_another_build_of_this_one_is_brought_to_this_macs_build() {
    let next = "Run atlas update to bring work's Atlas to this Mac's build.";
    for older in [
        facts(Some("a"), Some("0.1.9"), Some("b")),
        facts(Some("a"), Some("0.2.0"), Some("b")),
    ] {
        assert_eq!(older.verdict(), Verdict::Older, "{older:?}");
        assert_eq!(advice("work", &older), next);
        assert_eq!(
            said("work", &older),
            json!({"state": "available", "next": next})
        );
    }
}

#[test]
fn a_newer_guest_has_this_mac_updated_and_is_never_taken_back() {
    let newer = facts(Some("a"), Some("0.10.0"), Some("b"));

    assert_eq!(newer.verdict(), Verdict::Newer);
    assert_eq!(advice("work", &newer), "Update Atlas on this Mac.");
    assert_eq!(
        said("work", &newer),
        json!({"state": "newer", "next": "Update Atlas on this Mac."})
    );
}

#[test]
fn a_guest_that_takes_no_update_is_told_to_be_added_again_unless_it_is_newer_or_current() {
    let without = |mut facts: Facts| {
        facts.declared.retain(|name| name != UPDATES);
        facts
    };
    let remove = "work's Atlas cannot be updated from this Mac. Remove the VM and add it again.";

    let older = without(facts(Some("a"), Some("0.1.0"), Some("b")));
    assert_eq!(advice("work", &older), remove);
    assert_eq!(said("work", &older)["next"], remove);
    let unknown = without(facts(None, None, Some("b")));
    assert_eq!(advice("work", &unknown), remove);
    let newer = without(facts(Some("a"), Some("1.0.0"), Some("b")));
    assert_eq!(advice("work", &newer), "Update Atlas on this Mac.");
    let current = without(facts(Some("b"), Some("0.2.0"), Some("b")));
    assert_eq!(advice("work", &current), "Run atlas doctor --desktop work.");
}

#[test]
fn a_guest_whose_updates_are_not_bound_to_their_stage_is_never_offered_one() {
    let unbound = |mut facts: Facts| {
        facts.declared.retain(|name| name != UPDATE_BOUND);
        facts
    };
    let remove = "work's Atlas cannot be updated from this Mac. Remove the VM and add it again.";

    let older = unbound(facts(Some("a"), Some("0.1.0"), Some("b")));
    assert_eq!(advice("work", &older), remove);
    assert_eq!(said("work", &older)["next"], remove);
    let unknown = unbound(facts(None, None, Some("b")));
    assert_eq!(advice("work", &unknown), remove);
    assert_eq!(remedy("work", &unknown), remove);
    let bound = facts(Some("a"), Some("0.1.0"), Some("b"));
    assert_eq!(
        advice("work", &bound),
        "Run atlas update to bring work's Atlas to this Mac's build."
    );
}

#[test]
fn a_guest_whose_build_is_not_known_is_judged_by_its_version_alone() {
    let older = facts(None, Some("0.1.0"), Some("b"));
    let newer = facts(None, Some("0.3.0"), Some("b"));
    let same = facts(None, Some("0.2.0"), Some("b"));
    let neither = facts(Some("a"), Some("0.2.0"), None);

    assert_eq!(older.verdict(), Verdict::Older);
    assert_eq!(newer.verdict(), Verdict::Newer);
    assert_eq!(
        same.verdict(),
        Verdict::Unknown,
        "the same version may be this build"
    );
    assert_eq!(
        neither.verdict(),
        Verdict::Unknown,
        "this Mac's build not known"
    );
    assert_eq!(advice("work", &same), "Run atlas doctor --desktop work.");
    assert_eq!(said("work", &same), json!({"state": "unknown"}));
}

#[test]
fn a_guest_whose_version_is_not_known_or_not_a_version_is_current_only_by_its_build() {
    for unknown in [
        facts(Some("a"), None, Some("b")),
        facts(Some("a"), Some("0.2.0-beta"), Some("b")),
        facts(None, None, None),
    ] {
        assert_eq!(unknown.verdict(), Verdict::Unknown, "{unknown:?}");
        assert_eq!(advice("work", &unknown), "Run atlas doctor --desktop work.");
        assert_eq!(said("work", &unknown), json!({"state": "unknown"}));
    }
    assert_eq!(
        facts(Some("b"), None, Some("b")).verdict(),
        Verdict::Current
    );
}

#[test]
fn no_advice_or_status_a_vm_is_given_says_helper() {
    let mut every = Vec::new();
    for (digest, version) in [
        (Some("b"), Some("0.2.0")),
        (Some("a"), Some("0.1.0")),
        (Some("a"), Some("1.0.0")),
        (None, None),
    ] {
        for updatable in [true, false] {
            let mut judged = facts(digest, version, Some("b"));
            if !updatable {
                judged.declared.retain(|name| name != UPDATES);
            }
            every.push(advice("work", &judged));
            every.push(said("work", &judged).to_string());
        }
    }

    assert!(
        every
            .iter()
            .all(|said| !said.to_lowercase().contains("helper")),
        "{every:?}"
    );
}

#[test]
fn this_processs_own_build_is_its_executables_digest_and_its_version() {
    let own = Build::own();

    assert_eq!(
        own.digest,
        atlas_native::native::service::own_build(),
        "read once"
    );
    assert_eq!(own.version.as_deref(), Some(atlas_core::VERSION));
}

#[test]
fn doctor_never_sends_a_guest_back_to_doctor() {
    let restart = "Restart work with atlas desktop maintain work --restart; if it stays, remove it \
                   and add it again.";
    let without_updates = |mut facts: Facts| {
        facts.declared.retain(|name| name != UPDATES);
        facts
    };

    let current = facts(Some("b"), Some("0.2.0"), Some("b"));
    assert_eq!(remedy("work", &current), restart);
    assert_eq!(remedy("work", &without_updates(current)), restart);
    let unknown = facts(None, None, Some("b"));
    assert_eq!(remedy("work", &unknown), restart);
    assert_eq!(
        remedy("work", &without_updates(unknown)),
        "work's Atlas cannot be updated from this Mac. Remove the VM and add it again."
    );
    let older = facts(Some("a"), Some("0.1.0"), Some("b"));
    assert_eq!(remedy("work", &older), advice("work", &older));
    let newer = facts(Some("a"), Some("1.0.0"), Some("b"));
    assert_eq!(remedy("work", &newer), "Update Atlas on this Mac.");
}

#[test]
fn a_build_is_said_and_read_back_whole_and_one_said_by_nobody_is_not_known() {
    let build = Build::new(Some("b1"), Some("0.2.0"));

    assert_eq!(build.said(), json!({"build": "b1", "version": "0.2.0"}));
    assert_eq!(Build::of(&build.said()), build);
    assert!(build.known());
    assert!(!Build::of(&Value::Null).known());
    assert!(Build::new(None, Some("0.2.0")).known());
}

#[test]
fn a_status_says_an_update_its_keeper_owns_as_it_stands_and_the_copies_it_still_removes() {
    let judged = || json!({"state": "available", "next": "Run atlas update."});
    let target = json!({"build": "b1", "version": "0.2.0"});
    let left = json!([{"stage": "s", "removal": null}]);

    assert_eq!(with_owner(judged(), &Value::Null), judged());
    for phase in ["intended", "staging", "staged", "asked"] {
        assert_eq!(
            with_owner(
                judged(),
                &json!({"phase": phase, "target": target, "earlier": []})
            ),
            json!({"state": "updating", "phase": phase, "target": target}),
        );
    }
    assert_eq!(
        with_owner(
            json!({"state": "current"}),
            &json!({"phase": "cleaning", "target": target, "earlier": left,
                "outcome": {"ok": true, "update": "released"}}),
        ),
        json!({"state": "current", "phase": "cleaning",
            "outcome": {"ok": true, "update": "released"}, "earlier": left}),
    );
    assert_eq!(
        with_owner(
            json!({"state": "current"}),
            &json!({"phase": "done", "target": target, "earlier": [],
                "outcome": {"ok": true}}),
        ),
        json!({"state": "current"}),
        "a finished update leaves the judgement alone"
    );
    assert_eq!(
        with_owner(
            json!({"state": "current"}),
            &json!({"phase": "done", "target": target, "earlier": left, "outcome": {}}),
        ),
        json!({"state": "current", "phase": "done", "earlier": left}),
    );
    assert_eq!(
        with_owner(judged(), &json!({"phase": "unknown"})),
        json!({"state": "available", "next": "Run atlas update.", "phase": "unknown"}),
    );
}
