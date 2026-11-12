use super::*;

#[test]
fn every_worker_is_given_the_required_identity() {
    let expected = Identity {
        uid: 502,
        name: "helper".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let client = Client::new(config()).unwrap();
    for request in [
        Request::Inspect {
            uid: 502,
            labels: vec![LABEL.into()],
        },
        Request::Prepare {
            uid: 502,
            controller: "c".into(),
        },
        Request::Install {
            agent: Agent {
                label: LABEL.into(),
                program: "/Applications/Atlas.app/Contents/MacOS/Atlas".into(),
                uid: 502,
                controller_uid: 501,
                socket: "/Users/helper/.atlas/helper/502.sock".into(),
            },
        },
        Request::Retire {
            uid: 502,
            label: "old".into(),
            sha256: "a".repeat(64),
            missing_ok: false,
        },
        Request::Restore {
            uid: 502,
            label: "old".into(),
            sha256: "a".repeat(64),
        },
        Request::FirstLogin {
            uid: 502,
            opener: true,
        },
        Request::ForgetOpener { uid: 502 },
        Request::Kickstart { uid: 502 },
    ] {
        let argv = helper_argv(&client, &request, &expected).unwrap();
        assert_eq!(argv[3], "--account");
        assert_eq!(
            serde_json::from_str::<Identity>(&argv[4]).unwrap(),
            expected
        );
    }
}

#[tokio::test]
async fn update_preflight_inspects_only_the_managed_job_and_writes_no_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut migration = sandbox(dir.path());
    migration.log.state = State::Applied;
    migration.log.plan.home = dir.path().join("helper");
    let layout = migration.layout();
    let defined = migration.agent().plist_sha256(&layout.paths().logs());
    let plist = layout.agent_plist(LABEL).to_string_lossy().into_owned();
    let shown = std::cell::RefCell::new(json!({"loaded": false, "plistKind": "absent"}));
    let inspect = async |request| match request {
        Request::Inspect { labels, .. } => {
            assert_eq!(labels, vec![LABEL.to_owned()]);
            let job = shown.borrow().clone();
            Ok(json!({"ok": true, "jobs": {LABEL: job}}))
        }
        _ => panic!("preflight must only inspect"),
    };
    let program = Path::new("/updated/atlas");
    let signed = |path: &Path| {
        assert_eq!(path, program);
        Ok("updated-hash".into())
    };

    migration
        .preflight_with(program, signed, inspect)
        .await
        .unwrap();

    shown.replace(json!({
        "loaded": true, "plistKind": "regular", "plistSha256": defined, "path": plist,
    }));
    migration
        .preflight_with(program, signed, inspect)
        .await
        .unwrap();

    for differing in [
        json!({"loaded": true, "plistKind": "regular", "plistSha256": "b".repeat(64), "path": plist}),
        json!({"loaded": true, "plistKind": "regular", "plistSha256": defined, "path": "/elsewhere.plist"}),
        json!({"loaded": false, "plistKind": "link"}),
    ] {
        shown.replace(differing.clone());
        let error = migration
            .preflight_with(program, signed, inspect)
            .await
            .unwrap_err();
        assert!(
            error.error.contains("differs from its setup"),
            "{differing}: {}",
            error.error
        );
    }

    let unsigned = |_: &Path| Err(problem("not signed as Atlas"));
    let error = migration
        .preflight_with(program, unsigned, inspect)
        .await
        .unwrap_err();
    assert_eq!(error.error, "not signed as Atlas");

    let config_path = migration.log.plan.config_path.clone();
    shown.replace(json!({"loaded": false, "plistKind": "absent"}));
    atlas_core::config::keep_pictures(&config_path, false).unwrap();
    atlas_core::config::keep_days(&config_path, 3).unwrap();
    migration
        .preflight_with(program, signed, inspect)
        .await
        .expect("a setting that leaves the helper alone is no change to the setup");

    let other_helper = CONFIG.replace("60001", "60002");
    let moved_output = CONFIG.replace("\"runs\"", "\"elsewhere\"");
    for edited in ["edited", other_helper.as_str(), moved_output.as_str()] {
        fs::write(&config_path, edited).unwrap();
        let error = migration
            .preflight_with(program, signed, inspect)
            .await
            .unwrap_err();
        assert!(
            error
                .error
                .contains("changed during setup; it was not overwritten"),
            "{edited}"
        );
    }
    fs::write(&config_path, "edited").unwrap();
    let error = migration
        .preflight_with(program, signed, inspect)
        .await
        .unwrap_err();
    assert!(
        error
            .error
            .contains("changed during setup; it was not overwritten")
    );
    assert_eq!(fs::read_to_string(&config_path).unwrap(), "edited");

    assert!(!migration.path.exists(), "preflight writes no setup record");
}

fn config() -> Config {
    Config {
        helper_uid: 502,
        socket_path: "/tmp/atlas-live-502/502.sock".into(),
        identity_probe: "/Users/Shared/Atlas/atlas".into(),
        input_socket: "/tmp/atlas-input-502/input.sock".into(),
        output_dir: "/Users/c/runs".into(),
        activity: Default::default(),
    }
}

/// A first setup of the configuration `text` at `config_path`, as
/// `atlas init` records it.
fn plan(config_path: PathBuf, text: &str) -> Plan {
    Plan {
        uid: 502,
        controller: "c".into(),
        home: "/Users/h".into(),
        old_label: String::new(),
        old_plist_sha256: String::new(),
        label: LABEL.into(),
        controller_uid: 501,
        program: "/x".into(),
        cdhash: "ab".into(),
        config_path,
        old_config: text.into(),
        new_config: text.into(),
        fresh: true,
    }
}

/// A setup counts for the account at its home only: a record left by a
/// deleted account whose user ID was reused describes someone else.
#[test]
fn a_setup_is_in_effect_only_for_the_account_it_was_made_with() {
    let log = |state| {
        Ok(Some(Log {
            version: 1,
            plan: plan("/c/atlas.local.toml".into(), "a"),
            state,
            pending: Vec::new(),
            history: Vec::new(),
        }))
    };
    let home = Path::new("/Users/h");

    assert!(in_effect_with(log(State::Applied), home));
    assert!(in_effect_with(log(State::Applying), home));
    assert!(!in_effect_with(log(State::RolledBack), home));
    assert!(!in_effect_with(
        log(State::Applied),
        Path::new("/Users/new")
    ));
    assert!(!in_effect_with(Ok(None), home));
    assert!(
        in_effect_with(Err(problem("unreadable")), home),
        "a record that cannot be read counts"
    );
}

#[test]
fn logs_round_trip_privately() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep/setup-502.json");
    let log = Log {
        version: 1,
        plan: plan("/c/atlas.local.toml".into(), "a"),
        state: State::Applying,
        pending: vec!["g".into()],
        history: vec![("prepare".into(), "1".into())],
    };

    save_log(&path, &log).unwrap();
    assert_eq!(load_log(&path).unwrap(), Some(log));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(load_log(&dir.path().join("missing.json")).unwrap(), None);
}

/// The stored record keeps every field a setup has always written, and a
/// migration an earlier version recorded still reads, so it is refused by
/// name rather than as a malformed record.
#[test]
fn records_keep_the_format_setups_write() {
    let stored = json!({
        "version": 1,
        "plan": {
            "uid": 502,
            "controller": "c",
            "home": "/Users/h",
            "oldLabel": "",
            "oldPlistSha256": "",
            "label": LABEL,
            "controllerUid": 501,
            "program": "/x",
            "cdhash": "ab",
            "configPath": "/c/atlas.toml",
            "oldConfig": "a",
            "newConfig": "a",
            "fresh": true,
        },
        "state": "applied",
        "pending": ["g"],
        "history": [["prepare", "1"]],
    });
    let log: Log = serde_json::from_value(stored.clone()).unwrap();
    assert_eq!(log.plan, plan("/c/atlas.toml".into(), "a"));
    assert_eq!(serde_json::to_value(&log).unwrap(), stored);

    let mut migrated = stored;
    migrated["plan"]["oldLabel"] = json!("dev.atlas.live-sessiond");
    migrated["plan"]["oldPlistSha256"] = json!("a".repeat(64));
    migrated["plan"]["fresh"] = json!(false);
    migrated["state"] = json!("rolling-back");
    let log: Log = serde_json::from_value(migrated).unwrap();
    assert!(!log.plan.fresh);
    assert_eq!(log.state, State::RollingBack);
}

#[test]
fn plans_resolve_relative_configuration_against_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("atlas.local.toml");
    let text = "helper_uid = 502\nsocket_path = \"s.sock\"\nidentity_probe = \"p\"\n\
                    input_socket = \"i.sock\"\noutput_dir = \"runs\"\n";
    let log = Log {
        version: 1,
        plan: plan(config_path, text),
        state: State::Applying,
        pending: Vec::new(),
        history: Vec::new(),
    };

    let migration = Migration::new(dir.path().join("setup-502.json"), log).unwrap();
    assert_eq!(
        migration.client.config.socket_path,
        fs::canonicalize(dir.path()).unwrap().join("s.sock")
    );
}

/// Only a refusal the worker marks unchanged is certain; any other
/// failed result may have changed something.
#[test]
fn only_a_refusal_marked_unchanged_is_certain() {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let reply = |text: &str| {
        json!({"ok": true, "exitCode": 1, "events": [
            {"event": "output", "data": STANDARD.encode(text)}
        ]})
    };
    let refused = |reply: Value| {
        helper_reply(&reply)
            .is_ok_and(|value| matches!(acknowledged(value), Err(Unfinished::Unchanged(_))))
    };

    assert!(refused(reply(
        r#"{"ok":false,"error":"no","unchanged":true}"#
    )));
    assert!(!refused(reply(r#"{"ok":false,"error":"no"}"#)));
    assert!(!refused(reply(r#"{"ok":true,"unchanged":true}"#)));
    assert!(!refused(reply("garbage")));
    assert!(!refused(json!({"ok": true})));
}

/// Uninstall archives the record of the setup it removed; the archive
/// is never read as a setup.
#[test]
fn an_archived_record_is_no_longer_a_setup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("setup-502.json");
    assert!(!archive_as(&path, "uninstalled").unwrap(), "nothing there");

    let log = Log {
        version: 1,
        plan: plan("/c/atlas.toml".into(), "a"),
        state: State::Applied,
        pending: Vec::new(),
        history: Vec::new(),
    };
    save_log(&path, &log).unwrap();
    assert!(archive_as(&path, "uninstalled").unwrap());
    assert_eq!(load_log(&path).unwrap(), None);

    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1);
    let name = &names[0];
    assert!(
        name.starts_with("setup-502.") && name.ends_with(".uninstalled.json"),
        "{name}"
    );

    // install.sh counts only setup-<digits>.json as a live record.
    let uid = name
        .strip_prefix("setup-")
        .and_then(|rest| rest.strip_suffix(".json"))
        .unwrap();
    assert!(!uid.bytes().all(|byte| byte.is_ascii_digit()), "{uid}");
}

#[test]
fn helper_results_must_be_affirmative_json() {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let reply = |text: &str| {
        json!({"ok": true, "exitCode": 0, "events": [
            {"event": "output", "data": STANDARD.encode(text)}
        ]})
    };
    let result = |reply: Value| helper_reply(&reply).and_then(helper_result);

    assert_eq!(result(reply(r#"{"ok":true,"x":1}"#)).unwrap()["x"], 1);
    assert!(result(reply(r#"{"ok":false,"error":"no"}"#)).is_err());
    assert!(result(reply("garbage")).is_err());
    assert!(result(json!({"ok": true, "exitCode": 0})).is_err());
}

const CONFIG: &str = "helper_uid = 60001\nsocket_path = \"a.sock\"\nidentity_probe = \"p\"\n\
                          input_socket = \"i.sock\"\noutput_dir = \"runs\"\n";

/// A first setup of CONFIG kept entirely in `dir`: its configuration file,
/// its log and its pending-mutation state.
fn sandbox(dir: &Path) -> Migration {
    let config_path = dir.join("atlas.toml");
    fs::write(&config_path, CONFIG).unwrap();

    let log = Log {
        version: 1,
        plan: plan(config_path, CONFIG),
        state: State::Applying,
        pending: Vec::new(),
        history: Vec::new(),
    };

    let mut migration = Migration::new(dir.join("setup.json"), log).unwrap();
    migration.client.controller = atlas_control::Controller::at(60001, &dir.join("state"));

    migration
}

#[test]
fn uncertain_outcomes_stay_pending_and_definitive_ones_settle() {
    if !atlas_core::test_support::isolated(
        "setup::migrate::tests::uncertain_outcomes_stay_pending_and_definitive_ones_settle",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut migration = sandbox(dir.path());
    let step = |migration: &mut Migration| {
        let claim = migration.begin().unwrap();
        claim.to_string()
    };

    let uncertain = step(&mut migration);
    let outcome = migration.conclude(&uncertain, Err::<(), _>(problem("lost").uncertain()));
    assert!(outcome.unwrap_err().uncertain);

    let definitive = step(&mut migration);
    assert!(
        migration
            .conclude(&definitive, Err::<(), _>(problem("refused")))
            .is_err()
    );

    let succeeded = step(&mut migration);
    assert_eq!(migration.conclude(&succeeded, Ok(7)).unwrap(), 7);
    assert_eq!(migration.log.pending, vec![uncertain.clone()]);
    assert_eq!(
        migration.client.controller.pending().unwrap(),
        vec![uncertain.clone()]
    );

    let saved = load_log(&migration.path).unwrap().unwrap();
    assert_eq!(saved.pending, vec![uncertain]);
}

#[test]
fn only_pending_actions_this_setup_owns_let_it_continue() {
    if !atlas_core::test_support::isolated(
        "setup::migrate::tests::only_pending_actions_this_setup_owns_let_it_continue",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut migration = sandbox(dir.path());
    migration.require_owned_pending().unwrap();

    drop(migration.begin().unwrap());
    migration.require_owned_pending().unwrap();

    drop(migration.client.controller.quarantine().unwrap());
    let error = migration.require_owned_pending().unwrap_err();
    assert!(error.uncertain);
    assert_eq!(error.code, "uncertain");
}

fn recorded(state: State, fresh: bool, version: u32) -> Log {
    let mut planned = plan("/c/atlas.toml".into(), "a");
    planned.fresh = fresh;

    Log {
        version,
        plan: planned,
        state,
        pending: Vec::new(),
        history: Vec::new(),
    }
}

#[test]
fn apply_starts_only_from_a_log_it_can_continue() {
    let path = Path::new("/x/setup-502.json");
    assert_eq!(apply_start(None, path).unwrap(), Start::Plan(None));

    for state in [State::Applying, State::Applied] {
        let log = recorded(state, true, 1);
        assert_eq!(
            apply_start(Some(log.clone()), path).unwrap(),
            Start::Resume(log)
        );
    }

    for refused in [
        recorded(State::Applied, true, 2),
        recorded(State::RollingBack, true, 1),
        recorded(State::Applied, false, 1),
    ] {
        assert!(
            apply_start(Some(refused.clone()), path).is_err(),
            "{refused:?}"
        );
    }

    let replanned = recorded(State::RolledBack, true, 1);
    assert_eq!(
        apply_start(Some(replanned.clone()), path).unwrap(),
        Start::Plan(Some(replanned))
    );
}

#[test]
fn an_update_carries_over_only_a_setup_still_in_effect() {
    for state in [State::Applying, State::Applied] {
        assert!(carried_over(Ok(Some(recorded(state, true, 1)))));
    }

    for state in [State::RollingBack, State::RolledBack] {
        assert!(!carried_over(Ok(Some(recorded(state, false, 1)))));
    }

    assert!(!carried_over(Ok(None)));
    assert!(carried_over(Err(problem("unreadable"))));
}

#[test]
fn a_first_setup_resumes_its_own_log() {
    let path = Path::new("/x/setup-502.json");
    assert_eq!(fresh_start(None, path).unwrap(), Start::Plan(None));

    for state in [State::Applying, State::Applied, State::RolledBack] {
        let fresh = recorded(state, true, 1);
        assert_eq!(
            fresh_start(Some(fresh.clone()), path).unwrap(),
            Start::Resume(fresh)
        );
    }

    assert!(fresh_start(Some(recorded(State::Applied, true, 2)), path).is_err());
    assert!(fresh_start(Some(recorded(State::RollingBack, true, 1)), path).is_err());
}

/// A migration an earlier version of Atlas recorded is refused, in any
/// state, before a pass loads it, plans or archives anything, so the
/// actions it still owns stay pending. An unknown version is still named
/// as one.
#[test]
fn a_migration_an_earlier_version_recorded_is_refused_by_name() {
    let path = Path::new("/x/setup-502.json");

    for state in [
        State::Applying,
        State::Applied,
        State::RollingBack,
        State::RolledBack,
    ] {
        let mut migrated = recorded(state, false, 1);
        migrated.pending = vec!["g".into()];

        for refused in [
            apply_start(Some(migrated.clone()), path),
            fresh_start(Some(migrated), path),
        ] {
            let error = refused.unwrap_err();
            assert_eq!(error.error, earlier_migration(path).error, "{state:?}");
            assert!(error.error.contains("earlier version of Atlas"));
            assert!(error.error.contains("/x/setup-502.json"));
            assert!(!error.uncertain);
        }
    }

    let unknown = fresh_start(Some(recorded(State::Applied, false, 2)), path).unwrap_err();
    assert_eq!(unknown.error, unknown_version(path).error);
}
