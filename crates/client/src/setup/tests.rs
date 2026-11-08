#[test]
fn the_dispatcher_removes_first_login_copies_on_success_and_failure() {
    use std::os::unix::fs::PermissionsExt;

    for failure in ["none", "identity", "worker"] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Source.app/Contents/MacOS");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("atlas"), "worker").unwrap();
        fs::write(source.join("AtlasApp"), "app").unwrap();
        let guid = "6DB4B7AF-BE3C-486C-BD46-1B4558946919";
        let mut script = [crate::setup::executor::staging().as_str(), DISPATCH]
            .concat()
            .replace(
                "/private/var/tmp/atlas-approved.XXXXXXXX",
                &format!("{}/atlas-approved.XXXXXXXX", dir.path().display()),
            );
        for (tool, body) in [
            (
                "/usr/bin/dscl",
                format!(
                    "[[ $failure == identity && -e verified ]] && exit 1; print 'GeneratedUID: {guid}'"
                ),
            ),
            ("/usr/bin/id", "[[ $1 == -u ]] && print 7001 || print helper".into()),
            ("/usr/sbin/chown", "exit 0".into()),
            ("/usr/bin/codesign", "touch verified".into()),
            (
                "/usr/bin/sudo",
                "[[ -f $4 ]] || exit 2; print -r -- \"$4\" > worker-path; [[ $failure == worker ]] && exit 1; print '{\"ok\":true}'".into(),
            ),
        ] {
            let path = dir.path().join(Path::new(tool).file_name().unwrap());
            fs::write(&path, format!("#!/bin/zsh -f\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            script = script.replace(tool, path.to_str().unwrap());
        }
        let output = atlas_core::spawn::output(
            Command::new("/bin/zsh")
                .args(["-f", "-c", &script, "zsh", "7001", "helper", guid, "plain"])
                .arg(source.join("atlas"))
                .args([
                    r#"{"action":"first-login","uid":7001,"opener":true}"#,
                    "{}",
                    "--logged-out",
                ])
                .env("failure", failure)
                .current_dir(dir.path()),
        )
        .unwrap();
        assert_eq!(
            output.status.success(),
            failure == "none",
            "{failure}: {output:?}"
        );
        assert!(dir.path().join("verified").is_file());
        assert!(
            !fs::read_dir(dir.path()).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("atlas-approved.")
            }),
            "{failure}"
        );
        if failure != "identity" {
            let worker = fs::read_to_string(dir.path().join("worker-path")).unwrap();
            assert!(worker.contains("/atlas-approved."));
            assert!(!Path::new(worker.trim()).exists());
        }
    }
}

#[test]
fn the_privileged_dispatcher_checks_identity_before_entering_a_session() {
    use std::os::unix::fs::PermissionsExt;
    let identity = identity::Identity {
        uid: 7001,
        name: "helper".into(),
        generated_uid: "6DB4B7AF-BE3C-486C-BD46-1B4558946919".into(),
    };
    let requests = [
        helper_setup::Request::FirstLogin {
            uid: 7001,
            opener: false,
        },
        helper_setup::Request::Prepare {
            uid: 7001,
            controller: "person".into(),
        },
        helper_setup::Request::Install {
            agent: helper_setup::Agent {
                label: helper_setup::LABEL.into(),
                uid: 7001,
                controller_uid: 501,
                program: "/Applications/Atlas.app/Contents/MacOS/atlas".into(),
                socket: "/Users/helper/.atlas/helper/7001.sock".into(),
            },
        },
    ];
    for request in requests {
        for replacement in ["never", "before", "during"] {
            let dir = tempfile::tempdir().unwrap();
            let replaced = replacement != "never";
            if replacement == "before" {
                fs::write(dir.path().join("replaced"), "").unwrap();
            } else if replacement == "during" {
                fs::write(dir.path().join("replace-during-copy"), "").unwrap();
            }
            let mut script = [
                "stage_executable() { executable=/protected-atlas; if [[ -e replace-during-copy ]]; then touch replaced; fi; }\n",
                DISPATCH,
            ]
            .concat();
            for (tool, body) in [
                (
                    "/usr/bin/dscl",
                    format!(
                        "if [[ -e replaced ]]; then print 'GeneratedUID: 11111111-2222-3333-4444-555555555555'; else print 'GeneratedUID: {}'; fi",
                        identity.generated_uid
                    ),
                ),
                (
                    "/usr/bin/id",
                    "[[ $1 == -u ]] && print 7001 || print helper".into(),
                ),
                ("/bin/launchctl", "print -rl -- \"$@\" > dispatch".into()),
                ("/usr/bin/sudo", "print -rl -- \"$@\" > dispatch".into()),
            ] {
                let path = dir.path().join(Path::new(tool).file_name().unwrap());
                fs::write(&path, format!("#!/bin/zsh -f\n{body}\n")).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
                script = script.replace(tool, path.to_str().unwrap());
            }
            let encoded = serde_json::to_string(&identity).unwrap();
            let output = atlas_core::spawn::output(
                Command::new("/bin/zsh")
                    .args([
                        "-f",
                        "-c",
                        &script,
                        "zsh",
                        "7001",
                        "helper",
                        &identity.generated_uid,
                        "login",
                        "/unused-atlas",
                        &serde_json::to_string(&request).unwrap(),
                        &encoded,
                        "--",
                    ])
                    .current_dir(dir.path()),
            )
            .unwrap();
            assert_eq!(
                output.status.success(),
                !replaced,
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            let dispatched = fs::read_to_string(dir.path().join("dispatch"));
            if replaced {
                assert!(dispatched.is_err(), "no launchctl asuser or worker ran");
            } else {
                let dispatched = dispatched.unwrap();
                assert!(
                    dispatched.contains(&encoded),
                    "the worker receives the same expected identity"
                );
                assert!(dispatched.contains("/protected-atlas"));
                assert!(!dispatched.contains("/unused-atlas"));
            }
        }
    }
}

use super::*;

#[test]
fn disabled_services_parse_exactly() {
    let text = "disabled services = {\n\t\t\"com.apple.screensharing\" => disabled\n\
                    \t\t\"com.openssh.sshd\" => enabled\n\t}\n";
    assert_eq!(parse_disabled(text, "com.apple.screensharing"), Some(true));
    assert_eq!(parse_disabled(text, "com.openssh.sshd"), Some(false));
    assert_eq!(parse_disabled(text, "com.apple.screen"), None);
}

#[test]
fn endpoint_privacy_reflects_owner_mode_and_acl() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.path().join("socket");
    let me = nix::unistd::getuid().as_raw();

    let report = endpoint(&socket, me, me + 7);
    assert!(!report.present);
    assert_eq!(report.private, Some(true));
    assert_eq!(report.directory_acl, Some(Vec::new()));
    assert_eq!(endpoint(&socket, me + 1, me + 7).private, Some(false));

    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(endpoint(&socket, me, me + 7).private, Some(false));
}

#[test]
fn launchd_rows_and_paths_parse_exactly() {
    let list = "PID\tStatus\tLabel\n-\t0\tcom.apple.x\n68151\t0\tdev.atlas.live-sessiond\n\
                    6815\t0\tother\n";
    assert_eq!(
        job_label(list, 68151).as_deref(),
        Some("dev.atlas.live-sessiond")
    );
    assert_eq!(job_label(list, 6815).as_deref(), Some("other"));
    assert_eq!(job_label(list, 1), None);
    assert_eq!(job_label("68151\t0\ta\tb\n", 68151), None);

    let print = "gui/502/dev.atlas.live-sessiond = {\n\tactive count = 1\n\t\
                     path = /Users/atlastest/Library/LaunchAgents/dev.atlas.live-sessiond.plist\n}";
    assert_eq!(
        job_path(print),
        Some(PathBuf::from(
            "/Users/atlastest/Library/LaunchAgents/dev.atlas.live-sessiond.plist"
        ))
    );
    assert_eq!(job_path("\tpath = relative.plist\n"), None);
}

#[test]
fn only_helper_and_controller_grants_keep_a_directory_private() {
    let user = |id: u32, kind: &str| format!("user:UUID:name:{id}:{kind}:list,search");
    assert!(grants_only(&[], &[502, 501]));
    assert!(grants_only(
        &[user(501, "allow"), user(9, "deny")],
        &[502, 501]
    ));
    assert!(!grants_only(&[user(503, "allow")], &[502, 501]));
    assert!(!grants_only(
        &["group:UUID:everyone:12:allow:list".to_owned()],
        &[502, 501]
    ));
    assert!(!grants_only(&["garbage".to_owned()], &[502, 501]));
}

#[test]
fn preview_names_its_helper_or_says_how() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("atlas.toml");
    assert_eq!(read_config(&missing), (None, "missing", None));

    fs::write(&missing, "not = [toml").unwrap();
    let (config, state, error) = read_config(&missing);
    assert!(config.is_none() && state == "invalid" && error.is_some());

    assert_eq!(preview_account(None, None).unwrap_err().code, "usage");
    let unknown = preview_account(Some("no-such-atlas-account"), None).unwrap_err();
    assert_eq!(unknown.code, "setup");
}

#[test]
fn login_is_advised_only_for_a_confirmed_logout() {
    for absence in [Some("loggedOut"), Some("missing"), None] {
        let steps = next_steps(&json!({
            "session": {"reachable": false, "absence": absence},
        }));
        if absence == Some("loggedOut") {
            assert!(steps[0].contains("Log in"));
        } else {
            assert!(!steps[0].contains("Log in"), "{absence:?}");
            assert!(steps[0].contains("atlas init"), "{absence:?}");
        }
    }
}

#[tokio::test]
async fn observing_a_blocked_transport_preserves_its_code_and_advice() {
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::UnixListener,
    };

    let dir = tempfile::Builder::new()
        .prefix("atlas-")
        .tempdir_in("/tmp")
        .unwrap();
    let uid = 60001;
    let mut client = Client::new(Config {
        helper_uid: uid,
        socket_path: dir.path().join("helper.sock"),
        identity_probe: dir.path().join("atlas"),
        input_socket: dir.path().join("input.sock"),
        output_dir: dir.path().join("runs"),
        activity: Default::default(),
    })
    .unwrap();
    client.controller = atlas_control::Controller::at(uid, &dir.path().join("state"));
    client.transport.peer_uid = nix::unistd::getuid().as_raw();
    let listener = UnixListener::bind(&client.transport.path).unwrap();
    let status = json!({
        "ok": false,
        "code": "blocked",
        "error": "This shell may not connect to the helper; run atlas outside a sandbox.",
    });
    let serving = async {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut request = String::new();
        stream.read_line(&mut request).await.unwrap();
        stream
            .get_mut()
            .write_all(format!("{status}\n").as_bytes())
            .await
            .unwrap();
    };
    let mut report = json!({});
    tokio::join!(observe(&client, uid, &mut report), serving);

    let steps = next_steps(&report);
    let doctor = atlas_core::session::diagnosis(status, uid).value;
    assert_eq!(steps, vec![doctor["nextSteps"][0].as_str().unwrap()]);
    assert!(!steps.join(" ").contains("Log in"));
    assert_eq!(report["session"]["code"], "blocked");
}

#[test]
fn a_blocked_transport_advises_running_outside_the_sandbox() {
    for error in [
        json!("This shell may not connect to the helper; run atlas outside a sandbox."),
        Value::Null,
    ] {
        let steps = next_steps(&json!({
            "session": {"reachable": false, "code": "blocked", "error": error},
        }));
        assert_eq!(
            steps,
            ["This shell may not connect to the helper; run atlas outside a sandbox."]
        );
    }
}

#[test]
fn screen_sharing_is_never_a_setup_step() {
    for service in ["disabled", "unknown", "enabled"] {
        let steps = next_steps(&json!({"config": {"state": "present"},
            "session": {"reachable": true, "hidden": true},
            "screenSharing": {"service": service}}));
        assert!(!steps.join(" ").contains("Screen Sharing"), "{service}");
    }
}
