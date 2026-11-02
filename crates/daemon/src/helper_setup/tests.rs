#[test]
fn an_expected_identity_is_checked_again_even_without_a_logout_wait() {
    let identity = Identity {
        uid: 7001,
        name: "helper".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let account = Account {
        uid: identity.uid,
        name: identity.name.clone(),
        home: "/Users/helper".into(),
    };
    let guard = Guard {
        account: Some(identity.clone()),
        logged_out: false,
    };
    for observed in [
        None,
        Some(Identity {
            generated_uid: "11111111-2222-3333-4444-555555555555".into(),
            ..identity.clone()
        }),
    ] {
        let mut reads = 0;
        let error = checked_account(
            &guard,
            || {
                reads += 1;
                Ok((
                    account.clone(),
                    if reads == 1 {
                        Some(identity.clone())
                    } else {
                        observed.clone()
                    },
                ))
            },
            || panic!("no logout wait was requested"),
            || panic!("no logout check was requested"),
        )
        .unwrap_err();
        assert_eq!(reads, 2);
        assert!(!error.uncertain);
    }
    assert!(
        checked_account(
            &Guard {
                logged_out: true,
                ..guard
            },
            || Ok((account.clone(), Some(identity.clone()))),
            || false,
            || panic!("a failed logout wait refuses immediately"),
        )
        .is_err()
    );
}

#[test]
fn a_missing_identity_refuses_before_account_lookup_or_logout_waits() {
    for logged_out in [false, true] {
        let error = checked_account(
            &Guard {
                account: None,
                logged_out,
            },
            || panic!("a missing identity must refuse before account lookup"),
            || panic!("a missing identity must refuse before waiting"),
            || panic!("a missing identity must refuse before session inspection"),
        )
        .unwrap_err();
        assert!(
            error
                .error
                .contains("requires an expected account identity")
        );
        assert!(!error.uncertain);
    }
}

#[test]
fn every_worker_request_without_an_identity_refuses_before_dispatch() {
    let uid = nix::unistd::getuid().as_raw();
    let dir = tempfile::tempdir().unwrap();
    let layout = Layout::new(uid, dir.path().into());
    for request in [
        Request::Prepare {
            uid,
            controller: "controller".into(),
        },
        Request::Install {
            agent: Agent {
                label: LABEL.into(),
                program: "/bin/sh".into(),
                uid,
                controller_uid: uid + 1,
                socket: layout.socket(),
            },
        },
        Request::Retire {
            uid,
            label: "old".into(),
            sha256: "a".repeat(64),
            missing_ok: false,
        },
        Request::Restore {
            uid,
            label: "old".into(),
            sha256: "a".repeat(64),
        },
        Request::FirstLogin { uid, opener: true },
        Request::ForgetOpener { uid },
        Request::Kickstart { uid },
        Request::Inspect {
            uid,
            labels: vec![LABEL.into()],
        },
        Request::CheckRetire {
            uid,
            label: "old".into(),
            sha256: "a".repeat(64),
            missing_ok: false,
        },
    ] {
        for logged_out in [false, true] {
            let guard = Guard {
                account: None,
                logged_out,
            };
            let error = run_guarded_with(
                request.clone(),
                &guard,
                || {
                    Ok((
                        Account {
                            uid,
                            name: "helper".into(),
                            home: dir.path().into(),
                        },
                        None,
                    ))
                },
                || true,
                || true,
                |_, layout| {
                    fs::write(layout.home.join("mutation"), b"changed").unwrap();
                    panic!("no filesystem or launchd action may be dispatched")
                },
            )
            .unwrap_err();
            assert!(
                error
                    .error
                    .contains("requires an expected account identity"),
                "{request:?}: {error}"
            );
            assert!(!error.uncertain);
            let guarded = Layout {
                guard: Some(guard),
                ..Layout::new(uid, dir.path().into())
            };
            assert!(guarded.check().is_err());
        }
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn setup_refuses_an_account_replaced_while_authorization_or_login_was_pending() {
    let identity = Identity {
        uid: 7001,
        name: "helper".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let account = Account {
        uid: identity.uid,
        name: identity.name.clone(),
        home: "/Users/helper".into(),
    };
    let replacement = Identity {
        generated_uid: "11111111-2222-3333-4444-555555555555".into(),
        ..identity.clone()
    };
    let guard = Guard {
        account: Some(identity),
        logged_out: false,
    };
    let dir = tempfile::tempdir().unwrap();
    let result = checked_account(
        &guard,
        || Ok((account.clone(), Some(replacement.clone()))),
        || unreachable!(),
        || unreachable!(),
    );
    if result.is_ok() {
        fs::write(dir.path().join("mutation"), b"changed").unwrap();
    }
    assert!(result.is_err());
    assert!(!dir.path().join("mutation").exists());
}

#[test]
fn logout_verification_refreshes_identity_and_home_before_a_write() {
    use std::cell::Cell;
    let identity = Identity {
        uid: 7001,
        name: "helper".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let guard = Guard {
        account: Some(identity.clone()),
        logged_out: true,
    };
    let dir = tempfile::tempdir().unwrap();
    let account = Account {
        uid: identity.uid,
        name: identity.name.clone(),
        home: dir.path().join("original"),
    };
    let waited = Cell::new(false);
    let result = checked_account(
        &guard,
        || {
            let mut now = identity.clone();
            let mut account = account.clone();
            if waited.get() {
                now.generated_uid = "11111111-2222-3333-4444-555555555555".into();
                account.home = dir.path().join("replacement");
            }
            Ok((account, Some(now)))
        },
        || {
            waited.set(true);
            true
        },
        || true,
    );
    if let Ok(account) = &result {
        fs::write(&account.home, b"changed").unwrap();
    }
    assert!(result.is_err());
    assert!(!account.home.exists());
    assert!(!dir.path().join("replacement").exists());

    let fresh = checked_account(
        &guard,
        || {
            let mut account = account.clone();
            if waited.get() {
                account.home = dir.path().join("fresh-home");
            }
            Ok((account, Some(identity.clone())))
        },
        || true,
        || true,
    )
    .unwrap();
    assert_eq!(fresh.home, dir.path().join("fresh-home"));
    assert!(
        checked_account(
            &guard,
            || Ok((account.clone(), Some(identity.clone()))),
            || true,
            || false
        )
        .is_err(),
        "a login starting during the final account lookup is observed"
    );
}

use super::*;

#[test]
fn first_login_requests_read_back_as_written() {
    for request in [
        Request::FirstLogin {
            uid: 502,
            opener: false,
        },
        Request::ForgetOpener { uid: 502 },
    ] {
        let text = serde_json::to_string(&request).unwrap();
        assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    }
}

#[test]
fn names_are_conservative() {
    for name in ["dev.atlas.capture", "theiskaa", "a_b-c.1"] {
        assert!(valid_name(name), "{name}");
    }

    for name in [
        "",
        ".hidden",
        "-flag",
        "a/b",
        "a b",
        "../x",
        "a\nb",
        &"x".repeat(129),
    ] {
        assert!(!valid_name(name), "{name:?}");
    }
}

#[test]
fn plist_is_valid_and_escapes_text() {
    let agent = Agent {
        label: LABEL.into(),
        program: "/Users/Shared/A&B <x>/sessiond".into(),
        uid: 502,
        controller_uid: 501,
        socket: "/Users/h/.atlas/helper/502.sock".into(),
    };
    let text = plist(&agent, Path::new("/Users/h/.atlas/logs"));
    assert!(text.contains("<string>/Users/Shared/A&amp;B &lt;x&gt;/sessiond</string>"));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.plist");
    fs::write(&path, &text).unwrap();
    assert_eq!(file(&path).unwrap(), File::Regular(text_sha256(&text)));

    let lint = Command::new("/usr/bin/plutil")
        .arg("-lint")
        .arg(&path)
        .output()
        .unwrap();
    assert!(lint.status.success());

    let json = Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&path)
        .output()
        .unwrap();
    let parsed: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(parsed["Label"], LABEL);
    assert_eq!(
        parsed["ProgramArguments"][0],
        "/Users/Shared/A&B <x>/sessiond"
    );
    assert_eq!(parsed["ProgramArguments"][1], "helper-serve");
    assert_eq!(parsed["ProgramArguments"][3], "502");
    assert_eq!(parsed["ProgramArguments"][5], "501");
    assert_eq!(
        parsed["ProgramArguments"][7],
        "/Users/h/.atlas/helper/502.sock"
    );
    assert_eq!(parsed["ProcessType"], "Interactive");
    assert_eq!(parsed["KeepAlive"]["SuccessfulExit"], false);
    assert_eq!(parsed["LimitLoadToSessionType"], "Aqua");
}

#[test]
fn files_distinguish_absent_regular_and_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p");
    assert_eq!(file(&path).unwrap(), File::Absent);

    fs::write(&path, "abc").unwrap();
    assert!(matches!(file(&path).unwrap(), File::Regular(_)));

    let link = dir.path().join("l");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert_eq!(file(&link).unwrap(), File::Other);

    let dangling = dir.path().join("d");
    std::os::unix::fs::symlink(dir.path().join("missing"), &dangling).unwrap();
    assert_eq!(file(&dangling).unwrap(), File::Other);
    assert_eq!(file(dir.path()).unwrap(), File::Other);
}

#[test]
fn launchctl_print_needs_a_definition_path() {
    let text = "gui/502/x = {\n\tpath = /Users/h/Library/LaunchAgents/x.plist\n\
                    \tstate = running\n\tpid = 42\n}";
    assert_eq!(
        launchd::parse_job(text).unwrap(),
        Job::Loaded {
            pid: Some(42),
            path: "/Users/h/Library/LaunchAgents/x.plist".into()
        }
    );
    assert!(launchd::parse_job("gui/502/x = {\n\tstate = waiting\n}").is_err());
    assert!(launchd::parse_job("\tpath = relative.plist\n").is_err());
}

#[test]
fn acl_policy_allows_only_one_search_grant_for_the_controller() {
    let entry = |id: u32, kind: &str, rights: &str| format!("user:U:n:{id}:{kind}:{rights}");
    assert_eq!(acl_policy(&[], Some(501)), Ok(false));
    assert_eq!(
        acl_policy(&[entry(501, "allow", "search")], Some(501)),
        Ok(true)
    );
    assert_eq!(
        acl_policy(&[entry(501, "allow", "execute")], Some(501)),
        Ok(true)
    );

    for lines in [
        vec![entry(501, "allow", "list")],
        vec![entry(501, "allow", "search,list")],
        vec![entry(501, "allow", "search,file_inherit")],
        vec![entry(501, "deny", "search")],
        vec![entry(503, "allow", "search")],
        vec![entry(501, "allow", "search"), entry(501, "allow", "search")],
        vec!["group:U:everyone:12:allow:search".to_owned()],
    ] {
        assert!(acl_policy(&lines, Some(501)).is_err(), "{lines:?}");
    }

    assert!(acl_policy(&[entry(501, "allow", "search")], None).is_err());
}

#[test]
fn requests_are_typed_and_strict() {
    let request: Request = serde_json::from_str(
        r#"{"action":"retire","uid":502,"label":"dev.atlas.live-sessiond","sha256":"00","missing_ok":false}"#,
    )
    .unwrap();
    assert_eq!(request.uid(), 502);
    assert!(
        serde_json::from_str::<Request>(r#"{"action":"retire","uid":502,"label":"x"}"#).is_err()
    );
    assert!(serde_json::from_str::<Request>(r#"{"action":"delete","uid":502}"#).is_err());
    assert!(require_sha256("00").is_err());
    assert!(require_sha256(&"a".repeat(64)).is_ok());
}

#[test]
fn the_worker_refuses_another_account() {
    let other = nix::unistd::getuid().as_raw() + 1;
    let request = Request::Inspect {
        uid: other,
        labels: Vec::new(),
    };
    assert_eq!(
        run_guarded(request, &Guard::default()).unwrap_err().code,
        "setup"
    );
}

#[test]
fn install_accepts_only_the_managed_agent() {
    let layout = Layout::new(nix::unistd::getuid().as_raw(), "/nonexistent-home".into());

    let agent = |label: &str, socket: PathBuf| Agent {
        label: label.into(),
        program: "/bin/sh".into(),
        uid: layout.uid,
        controller_uid: 0,
        socket,
    };
    for agent in [
        agent("other.label", layout.socket()),
        agent(LABEL, layout.root().join("helper/../elsewhere.sock")),
        agent(LABEL, layout.daemon().join("other.sock")),
        agent(LABEL, "/tmp/x".into()),
    ] {
        let reply = install(&layout, &agent).unwrap();
        assert_eq!(reply["ok"], false);
        assert_eq!(
            reply["unchanged"], true,
            "a refusal before acting is certain"
        );
    }
}

/// A definition with other bytes (an earlier Atlas's) is refused
/// certainly, left as it was, and the refusal says what replaces it.
#[test]
fn install_refuses_another_definition_without_changing_it() {
    let (_dir, layout, _) = sandbox();
    let agent = Agent {
        label: LABEL.into(),
        program: "/bin/sh".into(),
        uid: layout.uid,
        controller_uid: 0,
        socket: layout.socket(),
    };

    let path = layout.agent_plist(LABEL);
    fs::write(&path, "an earlier definition").unwrap();

    let reply = install(&layout, &agent).unwrap();
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["unchanged"], true);
    assert!(
        reply["error"].as_str().unwrap().contains("atlas init"),
        "{reply}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "an earlier definition");
}

fn sandbox() -> (tempfile::TempDir, Layout, String) {
    let dir = tempfile::tempdir().unwrap();
    let layout = Layout::new(
        nix::unistd::getuid().as_raw(),
        fs::canonicalize(dir.path()).unwrap(),
    );
    fs::create_dir_all(layout.home.join("Library/LaunchAgents")).unwrap();
    // A label launchd has never seen: inspection is real and read-only.
    let label = format!("dev.atlas.test.{}", &atlas_core::files::token()[..12]);

    (dir, layout, label)
}

#[test]
fn retirement_preflight_and_retire_share_the_same_definition_and_backup_rules() {
    let dir = tempfile::tempdir().unwrap();
    let layout = Layout::new(60001, dir.path().into());
    let label = "old.helper";
    let sha256 = text_sha256("definition");
    let path = layout.agent_plist(label);
    let backup = layout.backup(label, &sha256);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::create_dir_all(backup.parent().unwrap()).unwrap();
    let check =
        |missing_ok| retirement_with(&layout, label, &sha256, missing_ok, || Ok(Job::Absent));
    assert!(check(false).is_err());
    assert!(check(true).is_ok());
    fs::write(&backup, "definition").unwrap();
    assert!(check(false).is_ok());
    fs::write(&path, "definition").unwrap();
    assert!(check(false).is_ok());
    fs::write(&backup, "other contents").unwrap();
    assert!(
        check(false).is_err(),
        "a conflicting backup refuses before unloading"
    );
    fs::remove_file(&backup).unwrap();
    assert!(check(false).is_ok());
    std::os::unix::fs::symlink(&path, &backup).unwrap();
    assert!(check(false).is_err(), "a backup link is never followed");
    fs::remove_file(&backup).unwrap();
    assert!(
        retirement_with(&layout, label, &sha256, false, || Ok(Job::Loaded {
            pid: Some(123),
            path: dir.path().join("foreign.plist")
        }))
        .is_err()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), "definition");
}

#[test]
fn retire_backs_up_exactly_the_expected_plist_and_repeats_its_receipt() {
    let (_dir, layout, label) = sandbox();
    prepare(&layout, "root").unwrap();
    let path = layout.agent_plist(&label);
    fs::write(&path, "definition").unwrap();
    let sha256 = text_sha256("definition");

    assert!(retire(&layout, &label, &"0".repeat(64), false).is_err());
    assert!(path.exists(), "a mismatched plist is left in place");

    let first = retire(&layout, &label, &sha256, false).unwrap();
    assert!(!path.exists());
    assert_eq!(
        fs::read_to_string(first["backup"].as_str().unwrap()).unwrap(),
        "definition"
    );
    assert_eq!(retire(&layout, &label, &sha256, false).unwrap(), first);

    let other = format!("{label}.other");
    assert!(
        retire(&layout, &other, &sha256, false).is_err(),
        "the original must be preserved"
    );
    assert_eq!(
        retire(&layout, &other, &sha256, true).unwrap()["absent"],
        true
    );
}

#[test]
fn retire_and_restore_never_touch_symlinks_or_other_contents() {
    let (_dir, layout, label) = sandbox();
    prepare(&layout, "root").unwrap();
    let path = layout.agent_plist(&label);
    let target = layout.home.join("real.plist");
    fs::write(&target, "definition").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let sha256 = text_sha256("definition");

    assert!(retire(&layout, &label, &sha256, true).is_err());
    assert!(restore(&layout, &label, &sha256).is_err());
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );

    fs::remove_file(&path).unwrap();
    assert!(
        restore(&layout, &label, &sha256).is_err(),
        "no backup to restore"
    );

    fs::write(&path, "changed").unwrap();
    assert!(restore(&layout, &label, &sha256).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "changed");
}

#[test]
fn prepare_creates_repairs_and_refuses_foreign_access() {
    let (_dir, layout, _) = sandbox();
    prepare(&layout, "root").unwrap();
    let capture = layout.daemon();
    let mode = |path: &Path| fs::metadata(path).unwrap().mode() & 0o777;
    assert_eq!(mode(&capture), 0o700);
    assert_eq!(acl_policy(&acl_lines(&capture).unwrap(), Some(0)), Ok(true));
    assert!(acl_lines(&layout.logs()).unwrap().is_empty());

    fs::set_permissions(&capture, fs::Permissions::from_mode(0o755)).unwrap();
    let repaired = prepare(&layout, "root").unwrap();
    assert_eq!(repaired["changed"], json!([capture]));
    assert_eq!(mode(&capture), 0o700);
    assert_eq!(
        acl_lines(&capture).unwrap().len(),
        1,
        "the grant is not duplicated"
    );

    let user = std::env::var("USER").unwrap();
    Command::new("/bin/chmod")
        .args(["+a", &format!("user:{user} allow list")])
        .arg(layout.logs())
        .status()
        .unwrap();
    fs::set_permissions(&capture, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(prepare(&layout, "root").is_err());
    assert_eq!(
        mode(&capture),
        0o755,
        "nothing changes when validation fails"
    );
}

/// Only a definition file with other bytes is replaced; none there (a
/// job from an older label) or the expected one is left alone.
#[test]
fn only_another_versions_definition_is_replaced() {
    let ours = "a".repeat(64);
    let earlier = "b".repeat(64);
    let job = |kind: &str, sha: Option<&str>| json!({"plistKind": kind, "plistSha256": sha});

    assert!(JobReport::from(&job("regular", Some(&earlier))).outdated(&ours));
    assert!(!JobReport::from(&job("regular", Some(&ours))).outdated(&ours));
    assert!(!JobReport::from(&job("absent", None)).outdated(&ours));
    assert!(
        !JobReport::from(&job("other", None)).outdated(&ours),
        "a link is never replaced"
    );
    assert!(!JobReport::from(&Value::Null).outdated(&ours));
}

#[test]
fn the_window_marker_names_a_living_window() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(ROOT);
    fs::create_dir_all(&root).unwrap();

    // A living process that is not Atlas's window does not count.
    fs::write(
        root.join(pairing::WINDOW_MARKER),
        format!("{}\n", std::process::id()),
    )
    .unwrap();
    assert!(!pairing::window_open(dir.path()));

    fs::write(root.join(pairing::WINDOW_MARKER), "999999\n").unwrap();
    assert!(!pairing::window_open(dir.path()));

    fs::remove_file(root.join(pairing::WINDOW_MARKER)).unwrap();
    assert!(!pairing::window_open(dir.path()));

    nix::unistd::mkfifo(
        &root.join(pairing::WINDOW_MARKER),
        nix::sys::stat::Mode::S_IRUSR,
    )
    .unwrap();
    assert!(!pairing::window_open(dir.path()));
}
