use atlas_client::test_support::converse_until;
use std::os::unix::net::UnixStream;

fn converse(stream: UnixStream, secret: Option<&[u8]>) -> (bool, Result<String, Refusal>) {
    converse_until(stream, secret, Instant::now() + APPROVAL)
}

#[test]
fn combined_setup_with_leftover_logins_requests_authorization_once() {
    for arguments in [
        vec![
            "create",
            "atlas",
            "/unused/Atlas",
            "7004",
            "Atlas",
            "picture",
            "7001",
            "7002",
        ],
        vec![
            "choose",
            "atlas",
            "/unused/Atlas",
            "7004",
            "identity",
            "7001",
            "7002",
        ],
    ] {
        // Intercept at the Authorization boundary: no dialog or privileged program can run.
        let ((result, ended), calls) = atlas_native::approval::without_authorization(|| {
            run_to_end(
                Via::Approval("Prepare the helper"),
                &arguments,
                Some(b"secret"),
            )
        });
        assert_eq!(calls, 1);
        assert_eq!(result, Err(Refusal::Unavailable));
        assert!(ended);
    }
}

#[test]
fn account_replacement_during_the_last_age_lookup_prevents_bootout() {
    let dir = tempfile::tempdir().unwrap();
    let mut tools = helper_tools(501, 7004, "opie");
    let body = tools.iter().find(|(tool, _)| *tool == "/usr/bin/dscl").unwrap().1.replace(
        "(accountPolicyData) print",
        "(accountPolicyData) n=0; [[ -e policies ]] && n=$(cat policies); (( n++ )); print $n > policies; (( n == 2 )) && touch reused; print",
    );
    replace(&mut tools, "/usr/bin/dscl", body);
    let target = Target::Leftover {
        helper: identity(7004, "opie"),
        pid: 4242,
    }
    .argument();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert!(
        text.contains("@atlas acting"),
        "replacement occurs after batch preflight: {text}"
    );
    assert!(text.contains("no longer the helper's account"), "{text}");
    assert!(!asked(dir.path(), "launchctl"), "{text}");
}

#[test]
fn a_callers_death_does_not_release_a_paused_executors_claims() {
    if !atlas_core::test_support::isolated(
        "onboarding::as_root::tests::a_callers_death_does_not_release_a_paused_executors_claims",
    ) {
        return;
    }
    for mode in ["approval", "command"] {
        let dir = tempfile::tempdir().unwrap();
        let mut fixture = executor_fixture(dir.path(), mode);
        fixture.ready().unwrap();
        fixture.interrupt().unwrap();
        let controller = atlas_control::Controller::at(7001, &dir.path().join("state"));
        let before = controller.pending().unwrap();
        let recovery_blocked = controller.acquire(true).is_err();
        controller.clear().unwrap();
        let after = controller.pending().unwrap();
        let normal_blocked = controller.acquire(false).is_err();
        fixture.finish().unwrap();
        assert!(!before.is_empty());
        assert!(
            recovery_blocked && normal_blocked,
            "a surviving executor blocks competing control: {mode}"
        );
        assert_eq!(
            after, before,
            "recovery preserves executor-backed pending work: {mode}"
        );
        assert!(dir.path().join("acted").exists());
        let held = controller.acquire(true).unwrap();
        controller.clear().unwrap();
        drop(held);
        assert!(controller.acquire(false).is_ok());
    }
}

fn executor_fixture(dir: &Path, mode: &str) -> atlas_client::test_support::Fixture {
    atlas_client::test_support::Fixture::spawn(
        dir,
        atlas_core::test_support::rerun(
            "onboarding::as_root::tests::a_privileged_executor_test_caller",
        )
        .arg("--nocapture")
        .env("ATLAS_EXECUTOR_TEST_DIRECTORY", dir)
        .env("ATLAS_EXECUTOR_TEST_MODE", mode)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null()),
    )
    .unwrap()
}

/// A separate caller process for the executor lifetime test. Without
/// its explicit temporary directory this is an inert test case.
#[test]
fn a_privileged_executor_test_caller() {
    use std::{os::fd::OwnedFd, process::Stdio};
    let Some(dir) = std::env::var_os("ATLAS_EXECUTOR_TEST_DIRECTORY") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let controller = atlas_control::Controller::at(7001, &dir.join("state"));
    // After the handshake, libtest output must survive a timed-out reply pipe.
    let script = [EXECUTOR, "exec \"$1\" --exact \"$2\" --nocapture >&2\n"].concat();
    let mode = std::env::var("ATLAS_EXECUTOR_TEST_MODE").unwrap();
    let mut command = Command::new("/bin/zsh");
    command
        .args(["-f", "-c", &script, "zsh"])
        .arg(std::env::current_exe().unwrap())
        .arg("onboarding::as_root::tests::a_privileged_executor_test_worker")
        .env("ATLAS_EXECUTOR_TEST_DIRECTORY", &dir);
    let outcome = super::super::owned(
        &[controller],
        || -> Result<(), Unclosed> {
            if mode == "command" {
                let _ = tracked_output(&mut command);
            } else {
                let (ours, theirs) = UnixStream::pair().unwrap();
                let input: OwnedFd = theirs.try_clone().unwrap().into();
                let output: OwnedFd = theirs.into();
                let mut child = atlas_core::spawn::spawn(
                    command
                        .stdin(Stdio::from(input))
                        .stdout(Stdio::from(output)),
                )
                .unwrap();
                let deadline = if mode == "timeout" {
                    Instant::now()
                } else {
                    Instant::now() + std::time::Duration::from_secs(30)
                };
                let (text, ended) = finish_executor(ours, None, deadline);
                let outcome = match text {
                    Ok(text) => closed_all(&text, &[Target::Orphan(7001)], ended),
                    Err(refusal) => Err(Unclosed {
                        refusal,
                        exit: Exit::of(true, ended),
                    }),
                };
                // finish_executor reaps the child through its reported identity.
                assert!(ended, "the executor's kernel exit must be observed");
                assert_eq!(child.wait().unwrap_err().raw_os_error(), Some(libc::ECHILD));
                return outcome.map(|_| ());
            }
            Ok(())
        },
        || true,
    );
    if mode == "timeout" {
        assert!(outcome.is_err());
    }
}

#[test]
fn a_privileged_executor_test_worker() {
    let Some(dir) = std::env::var_os("ATLAS_EXECUTOR_TEST_DIRECTORY") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    println!("@atlas acting");
    atlas_client::test_support::executor_ready(&dir, || {}).unwrap();
    fs::write(dir.join("acted"), b"").unwrap();
}

#[test]
fn a_timed_out_executor_holds_control_and_claims_until_it_exits() {
    if !atlas_core::test_support::isolated(
        "onboarding::as_root::tests::a_timed_out_executor_holds_control_and_claims_until_it_exits",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut fixture = executor_fixture(dir.path(), "timeout");
    fixture.ready().unwrap();
    let controller = atlas_control::Controller::at(7001, &dir.path().join("state"));
    let pending = controller.pending().unwrap();
    let busy = controller.acquire(true).is_err();
    controller.clear().unwrap();
    let after_clear = controller.pending().unwrap();
    let acted_early = dir.path().join("acted").exists();
    let early = fixture.caller.try_wait().unwrap();
    fixture.finish().unwrap();
    fixture.wait().unwrap();
    assert_eq!(pending.len(), 1);
    assert!(busy, "recovery cannot race the paused executor");
    assert_eq!(after_clear, pending, "recovery cannot erase its claim");
    assert!(!acted_early);
    assert!(
        early.is_none(),
        "the executor still could act when control was released"
    );
    assert!(dir.path().join("acted").exists());
    assert!(controller.pending().unwrap().is_empty());
    assert!(controller.acquire(true).is_ok());
}

#[test]
fn terminal_creation_never_runs_while_an_orphan_survives_cleanup() {
    for survives in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        replace(
            &mut tools,
            "/usr/bin/id",
            r#"print -r -- "id $*" >> calls
if [[ $1 == -u && -e created ]]; then print 7003; else exit 1; fi"#,
        );
        tools.push((
            "/usr/sbin/sysadminctl",
            r#"print -r -- "sysadminctl $*" >> calls; touch created"#.into(),
        ));
        let processes = tools
            .iter_mut()
            .find(|(tool, _)| *tool == "/usr/bin/pgrep")
            .unwrap();
        processes.1 = format!("[[ $2 == 7003 ]] && exit 1\n{}", processes.1);
        if survives {
            replace(&mut tools, "/usr/bin/pgrep", "exit 0");
            replace(&mut tools, "/usr/bin/pkill", "exit 0");
        }
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &[
                "create-terminal",
                "newhelper",
                "",
                "7003",
                "Atlas",
                "",
                "7001",
            ],
        );
        assert_eq!(
            asked(dir.path(), "sysadminctl -addUser"),
            !survives,
            "{text}"
        );
        assert_eq!(dir.path().join("created").exists(), !survives);
        if !survives {
            let calls = calls(dir.path());
            let closed = calls
                .iter()
                .position(|line| line == "launchctl bootout user/7001")
                .unwrap();
            let made = calls
                .iter()
                .position(|line| line.starts_with("sysadminctl -addUser"))
                .unwrap();
            assert!(closed < made);
        }
    }
}

#[test]
fn approved_creation_refuses_an_occupied_uid_or_a_failed_lookup() {
    refuses_occupied_uid("create");
}

#[test]
fn terminal_creation_refuses_an_occupied_uid_or_a_failed_lookup() {
    refuses_occupied_uid("create-terminal");
}

fn refuses_occupied_uid(mode: &str) {
    for failure in [
        "collision",
        "garbage",
        "lookup",
        "processes",
        "process-lookup",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        replace(&mut tools, "/usr/bin/id", "exit 1");
        replace(
            &mut tools,
            "/usr/bin/dscl",
            match failure {
                "collision" => r#"[[ $1 == /Search ]] && print 'directory-person 7003'; exit 0"#,
                "garbage" => "print garbage",
                "lookup" => "exit 3",
                _ => "exit 0",
            },
        );
        replace(
            &mut tools,
            "/usr/bin/pgrep",
            match failure {
                "processes" => "exit 0",
                "process-lookup" => "exit 3",
                _ => "exit 1",
            },
        );
        tools.push(("/usr/sbin/sysadminctl", "touch forbidden-creation".into()));
        tools.push(("/usr/bin/script", "touch forbidden-creation".into()));
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &[mode, "newhelper", "", "7003", "Atlas", ""],
        );
        assert!(text.contains("@atlas error"), "{mode} {failure}: {text}");
        assert!(
            !dir.path().join("forbidden-creation").exists(),
            "{mode} {failure}: {text}"
        );
    }
}

use super::*;
use atlas_daemon::helper_setup::Request;
use std::{fs, process::Command};

/// The steps with Apple's tools standing in as scripts in `dir` that
/// `tools` names; the stand-ins share `dir` for notes.
fn with_stand_ins(dir: &Path, tools: &[(&str, &str)]) -> String {
    let mut script = ["stage_executable() { :; }\n", STEPS].concat();
    for (tool, body) in tools {
        let path = dir.join(Path::new(tool).file_name().unwrap());
        fs::write(
            &path,
            format!("#!/bin/zsh -f\ncd {}\n{body}\n", dir.display()),
        )
        .unwrap();
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        script = script.replace(tool, &path.to_string_lossy());
    }

    script
}

/// The steps run as this account, with the stand-ins [`with_stand_ins`]
/// makes.
fn run_steps(dir: &Path, tools: &[(&str, &str)], arguments: &[&str]) -> String {
    let script = with_stand_ins(dir, tools);
    run_script(&script, arguments)
}

fn run_script(script: &str, arguments: &[&str]) -> String {
    let run = atlas_core::spawn::output(
        Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", script, "zsh"])
            .args(arguments),
    )
    .unwrap();
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// The helper's GeneratedUID in the stand-ins, and another account's.
const GUID: &str = "6DB4B7AF-BE3C-486C-BD46-1B4558946919";
const OTHER: &str = "11111111-2222-3333-4444-555555555555";

/// Stand-ins for the tools that look at and close logins, sharing notes
/// in their folder:
/// - the console is `on_screen`, or the user ID in `console` once it is
///   there;
/// - user ID N has an account while it is in `accounts` or `account-N`
///   is there;
/// - every login and process of a user ID runs until launchd boots it
///   out (`out-N`) or it is killed (`killed-N`), and its Dock outlives a
///   bootout while `dock-N` is there;
/// - its loginwindow is process 4242, or the one in `window-N`;
/// - listing processes fails while `broken` is there.
///
/// Each call is noted in `calls`.
fn login_tools(on_screen: u32, accounts: &[u32]) -> Vec<(&'static str, String)> {
    let accounts: Vec<String> = accounts.iter().map(u32::to_string).collect();
    let accounts = accounts.join(" ");
    vec![
        (
            "/usr/bin/stat",
            format!("[[ -e console ]] && cat console || print {on_screen}"),
        ),
        (
            "/usr/bin/id",
            format!(
                r#"print -r -- "id $*" >> calls
if [[ $1 == -nu ]]; then [[ " {accounts} " == *" $3 "* || -e account-$3 ]] && print someone || exit 1; fi"#
            ),
        ),
        (
            "/usr/bin/dscl",
            r#"print -r -- "dscl $*" >> calls"#.to_owned(),
        ),
        (
            "/bin/launchctl",
            r#"print -r -- "launchctl $*" >> calls; [[ $1 == bootout ]] && touch out-${2#*/}"#
                .to_owned(),
        ),
        (
            "/usr/bin/pgrep",
            r#"print -r -- "pgrep $*" >> calls
[[ -e broken ]] && exit 3
case $1 in
  (-qxu) [[ $3 == Dock && -e dock-$2 ]] && exit 0; [[ -e out-$2 ]] && exit 1 || exit 0 ;;
  (-xu) [[ -e out-$2 ]] && exit 1; [[ -e window-$2 ]] && cat window-$2 || print 4242 ;;
  (-qU) [[ -e killed-$2 ]] && exit 1 || exit 0 ;;
esac"#
                .to_owned(),
        ),
        (
            "/usr/bin/pkill",
            r#"print -r -- "pkill $*" >> calls; touch killed-${@[-1]}"#.to_owned(),
        ),
        ("/bin/sleep", "exit 0".to_owned()),
    ]
}

/// [`login_tools`] where user ID `uid` is the standard account `name`
/// with GeneratedUID [`GUID`] ([`OTHER`] once `reused` is there), until
/// `deleted` is there. The account was made at 999000, the clock reads
/// 1000000, and a login has run for what `etime` says, else an hour.
fn helper_tools(on_screen: u32, uid: u32, name: &str) -> Vec<(&'static str, String)> {
    let mut tools = login_tools(on_screen, &[]);
    replace(
        &mut tools,
        "/usr/bin/id",
        format!(
            r#"print -r -- "id $*" >> calls
case $1 in
  (-nu) [[ $3 == {uid} && ! -e deleted ]] && print {name} && exit 0
        [[ -e account-$3 ]] && print someone || exit 1 ;;
  (-u) [[ $3 == {name} && ! -e deleted ]] && print {uid} || exit 1 ;;
esac"#
        ),
    );
    replace(
        &mut tools,
        "/usr/bin/dscl",
        format!(
            r#"print -r -- "dscl $*" >> calls
case $4 in
  (GeneratedUID) [[ -e reused ]] && print "GeneratedUID: {OTHER}" || print "GeneratedUID: {GUID}" ;;
  (accountPolicyData) print "dsAttrTypeNative:accountPolicyData:"; print "<dict><key>creationTime</key>"; print "	<real>999000.25</real></dict>" ;;
esac"#
        ),
    );
    tools.push(("/usr/sbin/dseditgroup", "exit 67".to_owned()));
    tools.push((
        "/bin/ps",
        "[[ -e etime ]] && cat etime || print '  01:00:00'".to_owned(),
    ));
    tools.push(("/bin/date", "print 1000000".to_owned()));
    tools.push((
        "/usr/sbin/sysadminctl",
        r#"print -r -- "sysadminctl $*" >> calls; touch deleted"#.to_owned(),
    ));
    tools
}

/// Put `body` in place of the stand-in for `tool`.
fn replace(tools: &mut [(&'static str, String)], tool: &str, body: impl Into<String>) {
    let body = body.into();
    let found = tools
        .iter_mut()
        .find(|(name, _)| *name == tool)
        .unwrap_or_else(|| panic!("no stand-in for {tool}"));
    found.1 = body;
}

/// What the stand-ins were asked, in order.
fn calls(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Whether any call the stand-ins were asked starts with `start`.
fn asked(dir: &Path, start: &str) -> bool {
    calls(dir).iter().any(|call| call.starts_with(start))
}

fn borrowed<'a>(tools: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    tools
        .iter()
        .map(|(tool, body)| (*tool, body.as_str()))
        .collect()
}

fn me() -> u32 {
    nix::unistd::getuid().as_raw()
}

/// The helper `name` with user ID `uid`, as the stand-ins have it.
fn identity(uid: u32, name: &str) -> Identity {
    Identity {
        uid,
        name: name.to_owned(),
        generated_uid: GUID.to_owned(),
    }
}

#[test]
fn closing_refuses_the_screen_the_asker_an_account_and_a_system_user_id() {
    // A user ID other than this account's and the console's.
    let free = if me() == 7001 { 7002 } else { 7001 };
    let cases = [
        (free, vec![], free, format!("user ID {free} is on screen")),
        (
            me(),
            vec![],
            501,
            format!("user ID {} is the account asking", me()),
        ),
        (
            free,
            vec![free],
            501,
            format!("user ID {free} has an account"),
        ),
        (
            500,
            vec![],
            501,
            "user ID 500 is not a person's account".to_owned(),
        ),
    ];

    for (uid, accounts, on_screen, why) in cases {
        let dir = tempfile::tempdir().unwrap();
        let tools = login_tools(on_screen, &accounts);
        let target = Target::Orphan(uid).argument();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(closed(&text), Err(Refusal::Failed(why)), "{text}");
        assert!(
            !asked(dir.path(), "launchctl") && !asked(dir.path(), "pkill"),
            "nothing closed for {uid}"
        );
    }
}

#[test]
fn a_network_account_found_only_by_searching_every_directory_is_not_closed() {
    let dir = tempfile::tempdir().unwrap();
    let mut tools = login_tools(501, &[]);
    replace(
        &mut tools,
        "/usr/bin/dscl",
        "print 'network\t\tUniqueID = (\n    7001\n)'",
    );
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed("user ID 7001 has an account".into()))
    );
}

#[test]
fn every_user_id_is_checked_before_any_login_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    let tools = login_tools(501, &[7002]);
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["close", "orphan:7001", "orphan:7002"],
    );
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed("user ID 7002 has an account".into()))
    );
    assert!(!asked(dir.path(), "launchctl"));
}

#[test]
fn a_target_the_steps_do_not_know_is_refused_before_anything_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    let tools = helper_tools(501, 7004, "opie");
    for target in [
        "7001",
        "orphan:7001:x",
        "helper:7004:opie",
        "leftover:7004::GUID:1",
    ] {
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["close", "orphan:7003", target],
        );
        assert!(closed(&text).is_err(), "{target}: {text}");
    }
    assert!(!asked(dir.path(), "launchctl"));
}

#[test]
fn launchd_ends_a_deleted_accounts_login_before_anything_is_killed() {
    let dir = tempfile::tempdir().unwrap();
    let tools = login_tools(501, &[]);
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["close", "orphan:7001", "orphan:7003"],
    );
    assert_eq!(closed(&text), Ok(vec![7001, 7003]), "{text}");

    let calls = calls(dir.path());
    let at = |wanted: &str| {
        calls
            .iter()
            .position(|call| call == wanted)
            .unwrap_or_else(|| panic!("no {wanted} in {calls:?}"))
    };
    assert!(at("launchctl bootout user/7001") < at("pkill -TERM -U 7001"));
    assert!(at("pkill -TERM -U 7001") < at("launchctl bootout user/7003"));
    assert!(
        calls.iter().all(|call| !call.contains("KILL")),
        "stopped at the first ask"
    );
}

#[test]
fn a_login_that_outlives_its_stop_is_killed_and_one_that_outlives_that_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut tools = login_tools(501, &[]);
    // Only a kill ends what is left.
    replace(
        &mut tools,
        "/usr/bin/pkill",
        r#"print -r -- "pkill $*" >> calls; [[ $1 == -KILL ]] && touch killed-${@[-1]}"#,
    );
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(closed(&text), Ok(vec![7001]));
    assert!(calls(dir.path()).contains(&"pkill -KILL -U 7001".to_owned()));

    let dir = tempfile::tempdir().unwrap();
    replace(
        &mut tools,
        "/usr/bin/pkill",
        r#"print -r -- "pkill $*" >> calls"#,
    );
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed(
            "macOS didn't close the login of user ID 7001".into()
        ))
    );
}

#[test]
fn an_account_made_for_a_later_user_id_stops_the_steps_before_its_bootout() {
    let dir = tempfile::tempdir().unwrap();
    let mut tools = login_tools(501, &[]);
    // While 7001 closes, an account is made with user ID 7003.
    replace(
        &mut tools,
        "/bin/launchctl",
        r#"print -r -- "launchctl $*" >> calls; touch out-${2#*/} account-7003"#,
    );
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["close", "orphan:7001", "orphan:7003"],
    );
    assert!(text.contains("@atlas closed 7001"), "{text}");
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed("user ID 7003 has an account".into()))
    );
    assert!(!calls(dir.path()).contains(&"launchctl bootout user/7003".to_owned()));
}

#[test]
fn an_account_made_during_the_escalation_stops_the_next_kill() {
    // Made once launchd booted the login out: nothing is stopped.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = login_tools(501, &[]);
    replace(
        &mut tools,
        "/bin/launchctl",
        r#"print -r -- "launchctl $*" >> calls; touch account-7001"#,
    );
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed("user ID 7001 has an account".into()))
    );
    assert!(!asked(dir.path(), "pkill"));

    // Made once its processes were asked to stop: nothing is killed.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = login_tools(501, &[]);
    replace(
        &mut tools,
        "/usr/bin/pkill",
        r#"print -r -- "pkill $*" >> calls; touch account-7001"#,
    );
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed("user ID 7001 has an account".into()))
    );
    assert!(calls(dir.path()).contains(&"pkill -TERM -U 7001".to_owned()));
    assert!(!calls(dir.path()).contains(&"pkill -KILL -U 7001".to_owned()));
}

#[test]
fn a_login_that_comes_on_screen_is_never_killed_or_booted_out_again() {
    // A deleted account's login, on screen once launchd was asked.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = login_tools(501, &[]);
    replace(
        &mut tools,
        "/bin/launchctl",
        r#"print -r -- "launchctl $*" >> calls; print 7001 > console"#,
    );
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed("user ID 7001 is on screen".into()))
    );
    assert!(!asked(dir.path(), "pkill"));

    // The helper, on screen once its GUI session was booted out.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = helper_tools(501, 7004, "opie");
    replace(
        &mut tools,
        "/bin/launchctl",
        r#"print -r -- "launchctl $*" >> calls; print 7004 > console"#,
    );
    let target = Target::Helper(identity(7004, "opie")).argument();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed(
            "the helper is on screen; switch back to your own account first".into()
        ))
    );
    assert!(!calls(dir.path()).contains(&"launchctl bootout user/7004".to_owned()));
}

#[test]
fn who_is_on_screen_is_read_after_every_lookup_right_before_each_step() {
    // On screen while root looks the deleted account up a last time.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = login_tools(501, &[]);
    replace(
        &mut tools,
        "/usr/bin/dscl",
        r#"print -r -- "dscl $*" >> calls
print x >> looked; (( $(wc -l < looked) == 4 )) && print 7001 > console; exit 0"#,
    );
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed("user ID 7001 is on screen".into()))
    );
    assert!(!asked(dir.path(), "launchctl"));

    // The helper, on screen while root checks its account a last time.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = helper_tools(501, 7004, "opie");
    replace(
        &mut tools,
        "/usr/sbin/dseditgroup",
        "print x >> looked; (( $(wc -l < looked) == 2 )) && print 7004 > console; exit 67",
    );
    let target = Target::Helper(identity(7004, "opie")).argument();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed(
            "the helper is on screen; switch back to your own account first".into()
        ))
    );
    assert!(!asked(dir.path(), "launchctl"));
}

#[test]
fn a_login_that_starts_during_the_last_check_stops_the_deletion() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("out-7005"), "").unwrap();
    let mut tools = helper_tools(501, 7005, "opie");
    // Logged in again while root checks the account the second time.
    replace(
        &mut tools,
        "/usr/sbin/dseditgroup",
        "print x >> looked; (( $(wc -l < looked) == 2 )) && rm -f out-7005; exit 67",
    );
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["remove-helper", "7005", "opie", GUID],
    );
    assert_eq!(
        refused(&text),
        Err(Refusal::Failed("the helper is still logged in".into()))
    );
    assert!(!asked(dir.path(), "sysadminctl"));
}

#[test]
fn the_helpers_own_login_is_ended_through_launchd_and_never_killed() {
    let dir = tempfile::tempdir().unwrap();
    let mut tools = helper_tools(501, 7004, "opie");
    // The first bootout leaves it running.
    replace(
        &mut tools,
        "/bin/launchctl",
        r#"print -r -- "launchctl $*" >> calls; [[ $2 == user/* ]] && touch out-${2#*/}"#,
    );
    let target = Target::Helper(identity(7004, "opie")).argument();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(closed(&text), Ok(vec![7004]), "{text}");

    let calls = calls(dir.path());
    let gui = calls
        .iter()
        .position(|call| call == "launchctl bootout gui/7004");
    let user = calls
        .iter()
        .position(|call| call == "launchctl bootout user/7004");
    assert!(gui.is_some() && gui < user, "{calls:?}");
    assert!(calls.iter().all(|call| !call.starts_with("pkill")));
}

#[test]
fn the_helpers_login_is_never_closed_on_screen_or_as_an_administrator() {
    let target = Target::Helper(identity(7004, "opie")).argument();
    let dir = tempfile::tempdir().unwrap();
    let tools = helper_tools(7004, 7004, "opie");
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed(
            "the helper is on screen; switch back to your own account first".into()
        ))
    );

    let dir = tempfile::tempdir().unwrap();
    let mut tools = helper_tools(501, 7004, "opie");
    replace(&mut tools, "/usr/sbin/dseditgroup", "exit 0");
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed(
            "user ID 7004 is an administrator, or macOS could not tell".into()
        ))
    );
    assert!(!asked(dir.path(), "launchctl"));
}

#[test]
fn a_user_id_given_to_another_account_is_never_closed_or_deleted() {
    let no_longer = || Refusal::Failed("user ID 7004 is no longer the helper's account".into());
    let tools = helper_tools(501, 7004, "opie");
    let target = Target::Helper(identity(7004, "opie")).argument();

    // Another account had the user ID before the steps ran.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("reused"), "").unwrap();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(closed(&text), Err(no_longer()));
    fs::write(dir.path().join("out-7004"), "").unwrap();
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["remove-helper", "7004", "opie", GUID],
    );
    assert_eq!(refused(&text), Err(no_longer()));
    assert!(!asked(dir.path(), "launchctl") && !asked(dir.path(), "sysadminctl"));

    // A name that is not the account's own.
    let dir = tempfile::tempdir().unwrap();
    let named = Target::Helper(identity(7004, "ann")).argument();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &named]);
    assert_eq!(closed(&text), Err(no_longer()));

    // Another account took it while launchd ended the GUI session.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = helper_tools(501, 7004, "opie");
    replace(
        &mut tools,
        "/bin/launchctl",
        r#"print -r -- "launchctl $*" >> calls; touch reused"#,
    );
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(closed(&text), Err(no_longer()));
    assert!(calls(dir.path()).contains(&"launchctl bootout gui/7004".to_owned()));
    assert!(!calls(dir.path()).contains(&"launchctl bootout user/7004".to_owned()));
}

#[test]
fn a_leftover_login_is_closed_only_while_it_is_the_same_login_older_than_the_account() {
    let tools = helper_tools(501, 7004, "opie");
    let target = Target::Leftover {
        helper: identity(7004, "opie"),
        pid: 4242,
    }
    .argument();

    // An hour old, the account under twenty minutes: closed by launchd.
    let dir = tempfile::tempdir().unwrap();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(closed(&text), Ok(vec![7004]), "{text}");
    assert!(calls(dir.path()).contains(&"launchctl bootout gui/7004".to_owned()));
    assert!(!asked(dir.path(), "pkill"));

    // Another login of the user ID now.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("window-7004"), "5151\n").unwrap();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed("user ID 7004 has another login now".into()))
    );
    assert!(!asked(dir.path(), "launchctl"));

    // A login newer than the account: the helper's own.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("etime"), "   00:30\n").unwrap();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed(
            "the login of user ID 7004 is the helper's own".into()
        ))
    );
    assert!(!asked(dir.path(), "launchctl"));
}

#[test]
fn every_kind_of_login_closes_in_one_run() {
    let dir = tempfile::tempdir().unwrap();
    let tools = helper_tools(501, 7004, "opie");
    let leftover = Target::Leftover {
        helper: identity(7004, "opie"),
        pid: 4242,
    }
    .argument();
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["close", "orphan:7001", &leftover],
    );
    assert_eq!(closed(&text), Ok(vec![7001, 7004]), "{text}");
    assert!(calls(dir.path()).contains(&"pkill -TERM -U 7001".to_owned()));
    assert!(!calls(dir.path()).contains(&"pkill -TERM -U 7004".to_owned()));
}

#[test]
fn the_helper_is_deleted_only_once_logged_out_and_only_then_is_anything_killed() {
    let tools = helper_tools(501, 7005, "opie");
    let arguments = ["remove-helper", "7005", "opie", GUID];

    // Still logged in: nothing is deleted.
    let dir = tempfile::tempdir().unwrap();
    let text = run_steps(dir.path(), &borrowed(&tools), &arguments);
    assert!(
        text.contains("@atlas error the helper is still logged in"),
        "{text}"
    );
    assert!(!asked(dir.path(), "sysadminctl"));

    // Logged out: deleted, then what macOS left running is closed.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("out-7005"), "").unwrap();
    let text = run_steps(dir.path(), &borrowed(&tools), &arguments);
    assert!(text.contains("@atlas removed 7005"), "{text}");

    let calls = calls(dir.path());
    let deleted = calls
        .iter()
        .position(|call| call == "sysadminctl -deleteUser opie")
        .unwrap();
    let killed = calls
        .iter()
        .position(|call| call == "pkill -TERM -U 7005")
        .unwrap();
    assert!(deleted < killed, "{calls:?}");
}

#[test]
fn a_dock_left_running_is_not_logged_out() {
    // Its loginwindow ended, its Dock not: nothing is deleted.
    let dir = tempfile::tempdir().unwrap();
    let tools = helper_tools(501, 7005, "opie");
    fs::write(dir.path().join("out-7005"), "").unwrap();
    fs::write(dir.path().join("dock-7005"), "").unwrap();
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["remove-helper", "7005", "opie", GUID],
    );
    assert_eq!(
        refused(&text),
        Err(Refusal::Failed("the helper is still logged in".into()))
    );
    assert!(!asked(dir.path(), "sysadminctl"));

    // Ending the helper's login leaves its Dock: not ended.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("dock-7005"), "").unwrap();
    let target = Target::Helper(identity(7005, "opie")).argument();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
    assert_eq!(
        closed(&text),
        Err(Refusal::Failed(
            "macOS didn't end the login of user ID 7005".into()
        ))
    );
    assert!(!asked(dir.path(), "pkill"));
}

#[test]
fn a_process_list_that_fails_is_never_taken_for_none() {
    let failed = |uid| {
        Refusal::Failed(format!(
            "macOS could not list the processes of user ID {uid}"
        ))
    };
    let tools = helper_tools(501, 7005, "opie");

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("broken"), "").unwrap();
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["remove-helper", "7005", "opie", GUID],
    );
    assert_eq!(refused(&text), Err(failed(7005)));
    assert!(!asked(dir.path(), "sysadminctl"));

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("broken"), "").unwrap();
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(closed(&text), Err(failed(7001)));
    assert!(!asked(dir.path(), "pkill"));
}

#[test]
fn a_refusal_before_root_acts_is_told_from_one_after() {
    let one = [Target::Orphan(7001)];
    assert_eq!(
        closed_all("@atlas error user ID 7001 is on screen\n", &one, true),
        Err(Unclosed {
            refusal: Refusal::Failed("user ID 7001 is on screen".into()),
            exit: Exit::WithoutEffects,
        })
    );
    let late = "@atlas acting\n@atlas error macOS didn't close the login of user ID 7001\n";
    let late = closed_all(late, &one, true).unwrap_err();
    assert_eq!(late.exit, Exit::AfterEffects);
    let done = "@atlas acting\n@atlas closed 7001\n@atlas done\n";
    assert_eq!(closed_all(done, &one, true), Ok(vec![7001]));
    assert!(
        closed_all(done, &one, false).unwrap_err().exit == Exit::Unknown,
        "output alone is not termination"
    );
    let two = [Target::Orphan(7001), Target::Orphan(7003)];
    assert!(
        closed_all(done, &two, true).unwrap_err().exit == Exit::AfterEffects,
        "one never closed"
    );

    // Cut short before its last line: it may still be acting.
    let short = closed_all("@atlas acting\n@atlas closed 7001\n", &one, false).unwrap_err();
    assert_eq!(short.exit, Exit::Unknown);

    // The same reply from a run seen to exit afterwards: it ended.
    assert_eq!(
        closed_all("@atlas acting\n@atlas closed 7001\n", &one, true),
        Ok(vec![7001])
    );

    // The steps say so themselves.
    let dir = tempfile::tempdir().unwrap();
    let tools = login_tools(7001, &[]);
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert_eq!(
        closed_all(&text, &one, true).unwrap_err().exit,
        Exit::WithoutEffects,
        "{text}"
    );
    let dir = tempfile::tempdir().unwrap();
    let tools = login_tools(501, &[]);
    let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
    assert!(text.contains("@atlas acting"), "{text}");
    assert!(text.trim_end().ends_with("@atlas done"), "{text}");
}

#[test]
fn a_setup_run_is_told_untouched_from_one_that_may_have_acted() {
    assert_eq!(
        readied((Err(Refusal::Cancelled), true)),
        Err(untouched(Refusal::Cancelled))
    );
    assert_eq!(
        readied((Ok("@atlas readied {\"ok\": true}".into()), true)),
        Ok(())
    );

    assert_eq!(
        readied((Ok("@atlas readied {\"ok\": true}".into()), false))
            .unwrap_err()
            .exit,
        Exit::Unknown
    );

    let refused = readied((
        Ok("@atlas closed 7001\n@atlas error opie already exists\n".into()),
        true,
    ))
    .unwrap_err();
    assert_eq!(refused.exit, Exit::AfterEffects, "its executor exited");

    let lost = readied((
        Err(Refusal::Failed(
            "Atlas stopped waiting for the approved step, which may still finish".into(),
        )),
        false,
    ))
    .unwrap_err();
    assert_eq!(lost.exit, Exit::Unknown);
}

#[test]
fn targets_are_named_as_the_steps_read_them() {
    assert_eq!(Target::Orphan(7001).argument(), "orphan:7001");
    assert_eq!(
        Target::Helper(identity(502, "opie")).argument(),
        format!("helper:502:opie:{GUID}")
    );
    let leftover = Target::Leftover {
        helper: identity(502, "opie"),
        pid: 77,
    };
    assert_eq!(leftover.argument(), format!("leftover:502:opie:{GUID}:77"));
    assert_eq!(leftover.uid(), 502);
}

#[test]
fn readying_closes_deleted_accounts_logins_first() {
    let dir = tempfile::tempdir().unwrap();
    // The helper itself is logged out.
    fs::write(dir.path().join("out-502"), "").unwrap();
    let mut tools = login_tools(501, &[502]);
    replace(
        &mut tools,
        "/usr/bin/id",
        r#"print -r -- "id $*" >> calls
case $1 in
  (-nu) [[ $3 == 502 ]] && print opie || exit 1 ;;
  (-u) print 502 ;;
esac"#,
    );
    replace(&mut tools, "/usr/bin/dscl", GUID_TOOL);
    tools.push(("/usr/sbin/dseditgroup", "exit 67".to_owned()));
    tools.push(("/usr/sbin/createhomedir", "exit 0".to_owned()));
    tools.push((
        "/usr/bin/sudo",
        r#"print -r -- "sudo $*" >> calls; print -r -- '{"ok": true}'"#.to_owned(),
    ));
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["choose", "opie", EXECUTABLE, "502", GUID, "7001"],
    );
    assert_eq!(outcome(&text), Ok(()), "{text}");
    assert!(text.find("@atlas closed 7001") < text.find("@atlas readied"));

    let calls = calls(dir.path());
    let closed = calls
        .iter()
        .position(|call| call == "pkill -TERM -U 7001")
        .unwrap();
    let readied = calls
        .iter()
        .position(|call| call.starts_with("sudo"))
        .unwrap();
    assert!(closed < readied, "{calls:?}");
}

#[test]
fn a_login_that_cannot_be_closed_stops_the_creation() {
    let dir = tempfile::tempdir().unwrap();
    let mut tools = login_tools(501, &[7001]);
    tools.push((
        "/usr/sbin/sysadminctl",
        r#"print -r -- "sysadminctl $*" >> calls"#.to_owned(),
    ));
    let text = run_steps(
        dir.path(),
        &borrowed(&tools),
        &["create", "opie", EXECUTABLE, "502", "Opie", "", "7001"],
    );
    assert_eq!(
        outcome(&text),
        Err(Refusal::Failed("user ID 7001 has an account".into()))
    );
    assert!(!asked(dir.path(), "sysadminctl"));
}

#[test]
fn closing_is_read_from_its_own_lines() {
    assert_eq!(
        closed("@atlas closed 502\n@atlas closed 503\n"),
        Ok(vec![502, 503])
    );
    assert_eq!(
        closed("@atlas closed 502\n@atlas error macOS didn't close the login of user ID 503\n"),
        Err(Refusal::Failed(
            "macOS didn't close the login of user ID 503".into()
        ))
    );
    assert_eq!(
        closed(""),
        Err(Refusal::Failed("The step did not finish".into()))
    );
    assert_eq!(
        outcome("@atlas closed 7001\n@atlas readied {\"ok\": true}\n"),
        Ok(())
    );
    assert_eq!(
        outcome("@atlas closed 7001\n@atlas error opie already exists\n"),
        Err(Refusal::Failed("opie already exists".into()))
    );
}

/// A stand-in for dscl: the helper's GeneratedUID ([`OTHER`] once
/// `reused` is there), and no account found by any search.
const GUID_TOOL: &str = r#"print -r -- "dscl $*" >> calls
if [[ $4 == GeneratedUID ]]; then
  [[ -e reused ]] && print "GeneratedUID: 11111111-2222-3333-4444-555555555555" || print "GeneratedUID: 6DB4B7AF-BE3C-486C-BD46-1B4558946919"
fi
exit 0"#;

/// A stand-in for id: opie has user ID 502 (777 once `moved` is there).
const OPIE: &str = r#"print -r -- "id $*" >> calls
case $1 in
  (-nu) [[ $3 == 502 && ! -e moved ]] && print opie || exit 1 ;;
  (-u) [[ -e moved ]] && print 777 || print 502 ;;
esac"#;

const STANDARD: [(&str, &str); 5] = [
    ("/usr/sbin/dseditgroup", "exit 67"),
    (
        "/usr/sbin/createhomedir",
        r#"print -r -- "createhomedir $*" >> calls"#,
    ),
    ("/usr/bin/pgrep", "exit 1"),
    (
        "/usr/bin/sudo",
        r#"print -rl -- "$@" > sudo.log; print -r -- '{"ok": true}'"#,
    ),
    ("/usr/bin/dscl", GUID_TOOL),
];

const EXECUTABLE: &str = "/Applications/My Atlas $x *.app/Contents/MacOS/atlas";

#[test]
fn each_outcome_is_read_from_its_own_line() {
    let readied = "@atlas readied {\n  \"ok\": true,\n  \"answered\": []\n}\n";
    assert_eq!(outcome(readied), Ok(()));
    assert_eq!(outcome(&format!("@atlas created 1\n{readied}")), Ok(()));

    let refused = "User password:\r\n\
                       New account password error (eDSAuthPasswordTooShort)\r\n\
                       @atlas created 1\n@atlas error macOS did not create opie\n";
    assert_eq!(
        outcome(refused),
        Err(Refusal::Failed(
            "New account password error (eDSAuthPasswordTooShort)".into()
        ))
    );
    assert_eq!(
        outcome("@atlas error opie is an administrator, or macOS could not tell\n"),
        Err(Refusal::Failed(
            "opie is an administrator, or macOS could not tell".into()
        ))
    );
    assert_eq!(
        outcome("@atlas readied {\"ok\": false, \"error\": \"has Atlas settings\"}"),
        Err(Refusal::Failed("has Atlas settings".into()))
    );
    assert_eq!(
        outcome("@atlas readied sudo: unknown user opie"),
        Err(Refusal::Failed("sudo: unknown user opie".into()))
    );
    assert!(outcome("@atlas created 0\n").is_err(), "it never finished");

    let joined =
        "eDSRecordAlreadyExists\r@atlas created 72\n@atlas error macOS did not create opie\n";
    assert_eq!(
        outcome(joined),
        Err(Refusal::Failed("eDSRecordAlreadyExists".into())),
        "sysadminctl's last words without a newline"
    );
    assert_eq!(
        outcome("User password:\r\n\n@atlas created 72\n@atlas error macOS did not create opie\n"),
        Err(Refusal::Failed(
            "macOS did not create opie (sysadminctl stopped with status 72)".into()
        )),
        "nothing said but its status"
    );
}

#[test]
fn a_password_that_appears_in_the_steps_lines_leaves_them_whole() {
    let (ours, theirs) = UnixStream::pair().unwrap();
    let writer = std::thread::spawn(move || {
        let mut theirs = theirs;
        std::io::Write::write_all(
            &mut theirs,
            b"atlas\r\n@atlas created 0\n@atlas readied {\"ok\": true}\n",
        )
        .unwrap();
    });
    let (finished, text) = converse(ours, Some(b"atlas"));
    let text = text.unwrap();
    assert!(finished);
    writer.join().unwrap();
    assert!(!text.starts_with("atlas"), "cut from sysadminctl's part");
    assert_eq!(outcome(&text), Ok(()));
}

#[test]
fn readying_asks_the_helper_for_its_first_login_as_setup_helper_reads_it() {
    let request = serde_json::to_string(&Request::FirstLogin {
        uid: 502,
        opener: true,
    })
    .unwrap();
    let written = request.replace('"', "\\\"").replace("502", "$uid");
    assert!(
        STEPS.contains(&format!("request=\"{written}\"")),
        "{written}"
    );
    assert!(STEPS.contains(&format!("<{}-", atlas_core::account::FIRST_USER_UID)));
}

#[test]
fn the_steps_are_valid_zsh_and_stop_without_an_administrator() {
    let script = [BECOME_ROOT, &steps()].concat();
    let check =
        atlas_core::spawn::output(Command::new("/bin/zsh").args(["-n", "-f", "-c", &script]))
            .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let run = atlas_core::spawn::output(
        Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args(["choose", "nobody", "/nowhere", "501"]),
    )
    .unwrap();
    assert_eq!(
        outcome(&String::from_utf8_lossy(&run.stdout)),
        Err(Refusal::Failed(
            "macOS did not let Atlas act as an administrator".into()
        ))
    );
}

#[test]
fn the_worker_runs_the_verified_copy_even_when_the_source_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("Source.app/Contents/MacOS");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("atlas"), "verified").unwrap();
    fs::write(source.join("AtlasApp"), "verified app").unwrap();
    let tools = [
        ("/usr/bin/id", OPIE),
        ("/usr/sbin/chown", "print -rl -- \"$@\" > ownership"),
        (
            "/usr/bin/codesign",
            r#"target=${@[-1]}
[[ $(/usr/bin/stat -f %Lp ${target%/Atlas.app*}) == 700 ]] || exit 1
if [[ $target == *.app ]]; then
  [[ $(<$target/Contents/MacOS/atlas) == verified ]] || exit 1
  print replaced > Source.app/Contents/MacOS/atlas
fi
print -r -- "$*" >> verification"#,
        ),
        (
            "/usr/bin/sudo",
            r#"print -r -- "$4" > worker-path
cat "$4" > worker-bytes
print -r -- '{"ok": true}'"#,
        ),
    ];
    let mut tools = tools.to_vec();
    tools.extend(
        STANDARD
            .into_iter()
            .filter(|(tool, _)| !matches!(*tool, "/usr/bin/sudo")),
    );
    let script = staged_stand_ins(dir.path(), &tools);
    let text = run_script(
        &script,
        &[
            "choose",
            "opie",
            source.join("atlas").to_str().unwrap(),
            "502",
            GUID,
        ],
    );
    assert_eq!(outcome(&text), Ok(()), "{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("worker-bytes")).unwrap(),
        "verified"
    );
    assert_eq!(
        fs::read_to_string(source.join("atlas")).unwrap(),
        "replaced\n"
    );
    let worker = fs::read_to_string(dir.path().join("worker-path")).unwrap();
    assert!(worker.starts_with(dir.path().to_str().unwrap()));
    assert!(worker.contains("/atlas-approved."));
    assert!(!Path::new(worker.trim()).exists());
    assert_no_staged_copies(dir.path());
    let ownership = fs::read_to_string(dir.path().join("ownership")).unwrap();
    assert!(ownership.starts_with("-R\n0:0\n"));
    let verified = fs::read_to_string(dir.path().join("verification")).unwrap();
    assert!(verified.contains(&atlas_core::app_bundle::requirement_for(
        atlas_core::app_bundle::IDENTIFIER
    )));
    assert!(verified.contains(&atlas_core::app_bundle::requirement_for(
        atlas_core::app_bundle::APP_IDENTIFIER
    )));
}

#[test]
fn every_failure_after_staging_removes_the_copy() {
    for failure in [
        "permissions",
        "copy",
        "ownership",
        "opening",
        "account",
        "worker",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Source.app/Contents/MacOS");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("atlas"), "verified").unwrap();
        fs::write(source.join("AtlasApp"), "verified app").unwrap();
        let mut tools = vec![
            ("/usr/bin/id", OPIE),
            ("/usr/sbin/chown", "exit 0"),
            ("/usr/bin/codesign", "exit 0"),
        ];
        tools.extend(STANDARD);
        match failure {
            "permissions" => tools.push(("/bin/chmod", "exit 1")),
            "copy" => tools.push(("/usr/bin/ditto", "exit 1")),
            "ownership" => tools[1].1 = "exit 1",
            "opening" => tools.push(("/bin/chmod", "[[ $1 == 755 ]] && exit 1; /bin/chmod \"$@\"")),
            "account" => tools[0].1 = "exit 1",
            "worker" => {
                let sudo = tools
                    .iter_mut()
                    .find(|(tool, _)| *tool == "/usr/bin/sudo")
                    .unwrap();
                sudo.1 = "print -r -- '{\"ok\":false,\"error\":\"worker failed\"}'; exit 1";
            }
            _ => unreachable!(),
        }
        let script = staged_stand_ins(dir.path(), &tools);
        let text = run_script(
            &script,
            &[
                "choose",
                "opie",
                source.join("atlas").to_str().unwrap(),
                "502",
                GUID,
            ],
        );
        assert!(outcome(&text).is_err(), "{failure}: {text}");
        assert_no_staged_copies(dir.path());
    }
}

#[test]
fn a_closed_reply_pipe_after_staging_still_removes_the_copy() {
    use std::{
        io::{BufRead, BufReader, Write},
        process::Stdio,
    };

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("Source.app/Contents/MacOS");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("atlas"), "verified").unwrap();
    fs::write(source.join("AtlasApp"), "verified app").unwrap();
    let script = staged_stand_ins(
        dir.path(),
        &[
            ("/usr/bin/id", "exit 1"),
            ("/usr/sbin/chown", "exit 0"),
            ("/usr/bin/codesign", "exit 0"),
            (
                "/bin/chmod",
                r#"if [[ $1 == 755 ]]; then
  print -r -- "$2"
  IFS= read -r acknowledgement || exit 1
fi
/bin/chmod "$@""#,
            ),
        ],
    );
    let mut child = atlas_core::spawn::spawn(
        Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args([
                "choose",
                "opie",
                source.join("atlas").to_str().unwrap(),
                "502",
                GUID,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .unwrap();
    let mut reply = BufReader::new(child.stdout.take().unwrap());
    let mut staged = String::new();
    reply.read_line(&mut staged).unwrap();
    assert!(Path::new(staged.trim()).join("Atlas.app").exists());
    drop(reply);
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    let status = child.wait().unwrap();
    assert_no_staged_copies(dir.path());
    assert_eq!(status.code(), Some(1));
}

fn assert_no_staged_copies(dir: &Path) {
    assert!(!fs::read_dir(dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("atlas-approved.")
    }));
}

#[test]
fn an_invalid_or_linked_copy_never_reaches_the_worker() {
    for linked in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Source.app/Contents/MacOS");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("atlas"), "replacement").unwrap();
        if linked {
            std::os::unix::fs::symlink("/bin/ls", source.join("AtlasApp")).unwrap();
        } else {
            fs::write(source.join("AtlasApp"), "replacement app").unwrap();
        }
        let mut tools = vec![
            ("/usr/bin/id", OPIE),
            ("/usr/sbin/chown", "exit 0"),
            ("/usr/bin/codesign", "exit 1"),
        ];
        tools.extend(STANDARD);
        let script = staged_stand_ins(dir.path(), &tools);
        let text = run_script(
            &script,
            &[
                "choose",
                "opie",
                source.join("atlas").to_str().unwrap(),
                "502",
                GUID,
            ],
        );
        if linked {
            assert_eq!(
                outcome(&text),
                Err(Refusal::Failed(
                    "Atlas's copy contains links or special files".into()
                )),
                "{text}"
            );
        } else {
            assert!(outcome(&text).is_err(), "{text}");
        }
        assert!(!dir.path().join("sudo.log").exists());
        assert!(!asked(dir.path(), "createhomedir"));
        assert_no_staged_copies(dir.path());
    }
}

fn staged_stand_ins(dir: &Path, tools: &[(&str, &str)]) -> String {
    let staging = steps().strip_suffix(STEPS).unwrap().to_owned();
    let mut script =
        with_stand_ins(dir, tools).replacen("stage_executable() { :; }\n", &staging, 1);
    // Staging also uses tool stand-ins, and all copies stay in the fixture.
    for (tool, _) in tools {
        script = script.replace(
            tool,
            &dir.join(Path::new(tool).file_name().unwrap())
                .to_string_lossy(),
        );
    }
    script.replace(
        "/private/var/tmp/atlas-approved.XXXXXXXX",
        &format!("{}/atlas-approved.XXXXXXXX", dir.display()),
    )
}

#[test]
fn readying_hands_names_and_paths_to_each_tool_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let mut tools = vec![("/usr/bin/id", OPIE)];
    tools.extend(STANDARD);
    let text = run_steps(
        dir.path(),
        &tools,
        &["choose", "opie", EXECUTABLE, "502", GUID],
    );

    assert_eq!(outcome(&text), Ok(()), "{text}");
    assert_eq!(
        fs::read_to_string(dir.path().join("sudo.log"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "-n",
            "-u",
            "opie",
            EXECUTABLE,
            "setup-helper",
            r#"{"action":"first-login","uid":502,"opener":true}"#,
            "--account",
            r#"{"uid":502,"name":"opie","generatedUid":"6DB4B7AF-BE3C-486C-BD46-1B4558946919"}"#,
            "--logged-out",
        ]
    );
}

#[test]
fn an_account_that_changes_after_it_was_checked_is_never_readied() {
    // Its user ID moves while root checks whether it is standard.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = vec![
        ("/usr/bin/id", OPIE),
        ("/usr/sbin/dseditgroup", "touch moved; exit 67"),
    ];
    tools.extend(STANDARD.into_iter().skip(1));
    let text = run_steps(
        dir.path(),
        &tools,
        &["choose", "opie", EXECUTABLE, "502", GUID],
    );
    assert_eq!(
        outcome(&text),
        Err(Refusal::Failed(
            "opie does not have the user ID Atlas checked".into()
        ))
    );
    assert!(!asked(dir.path(), "createhomedir"));

    // Another account than the one checked, with the same name and ID.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("reused"), "").unwrap();
    let mut tools = vec![("/usr/bin/id", OPIE)];
    tools.extend(STANDARD);
    let text = run_steps(
        dir.path(),
        &tools,
        &["choose", "opie", EXECUTABLE, "502", GUID],
    );
    assert_eq!(
        outcome(&text),
        Err(Refusal::Failed(
            "opie is not the account Atlas checked".into()
        ))
    );
    assert!(!asked(dir.path(), "createhomedir"));

    // Replaced once its home was made: it is not readied as the helper.
    let dir = tempfile::tempdir().unwrap();
    let mut tools = vec![
        ("/usr/bin/id", OPIE),
        (
            "/usr/sbin/createhomedir",
            r#"print -r -- "createhomedir $*" >> calls; touch reused"#,
        ),
    ];
    tools.extend(
        STANDARD
            .into_iter()
            .filter(|(tool, _)| *tool != "/usr/sbin/createhomedir"),
    );
    let text = run_steps(
        dir.path(),
        &tools,
        &["choose", "opie", EXECUTABLE, "502", GUID],
    );
    assert_eq!(
        outcome(&text),
        Err(Refusal::Failed(
            "opie is not the account Atlas checked".into()
        ))
    );
    assert!(!dir.path().join("sudo.log").exists());
}

#[test]
fn an_account_whose_user_id_changed_since_it_was_checked_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut tools = vec![("/usr/bin/id", OPIE)];
    tools.extend(STANDARD);
    let text = run_steps(
        dir.path(),
        &tools,
        &["choose", "opie", EXECUTABLE, "503", GUID],
    );

    assert_eq!(
        outcome(&text),
        Err(Refusal::Failed(
            "opie does not have the user ID Atlas checked".into()
        ))
    );
    assert!(!dir.path().join("sudo.log").exists());
}

#[test]
fn a_creation_that_fails_silently_reports_sysadminctls_real_status() {
    let dir = tempfile::tempdir().unwrap();
    let mut script = ["stage_executable() { :; }\n", STEPS].concat();
    let tools = [
        ("/usr/bin/id", "exit 1"),
        ("/usr/bin/dscl", "exit 0"),
        ("/usr/bin/pgrep", "exit 1"),
        ("/usr/sbin/sysadminctl", "exit 3"),
    ];
    for (tool, body) in tools {
        let path = dir.path().join(Path::new(tool).file_name().unwrap());
        fs::write(&path, format!("#!/bin/zsh -f\n{body}\n")).unwrap();
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        script = script.replace(tool, &path.to_string_lossy());
    }

    let (ours, theirs) = UnixStream::pair().unwrap();
    let mut child = atlas_core::spawn::spawn(
        Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args(["create", "opie", EXECUTABLE, "502", "Opie", ""])
            .stdin(std::os::fd::OwnedFd::from(theirs.try_clone().unwrap()))
            .stdout(std::os::fd::OwnedFd::from(theirs)),
    )
    .unwrap();
    let (_, text) = converse(ours, Some(b"secret"));
    child.wait().unwrap();

    assert_eq!(
        outcome(&text.unwrap()),
        Err(Refusal::Failed(
            "macOS did not create opie (sysadminctl stopped with status 3)".into()
        ))
    );
}

#[test]
fn creating_types_the_password_into_sysadminctl_over_the_approved_steps_socket() {
    let dir = tempfile::tempdir().unwrap();
    // The real script, on a socket as the approval gives it; sysadminctl
    // stands in, asking as it does and noting what it got.
    let mut tools = vec![
        (
            "/usr/bin/id",
            r#"[[ -e made ]] || exit 1; [[ $1 == -nu ]] && print opie || print 502"#,
        ),
        (
            "/usr/sbin/sysadminctl",
            r#"print -rl -- "$@" > sysadminctl.log; cp "${@[-1]}" picture.png
read -rs "password?User password:"; print -rn -- $password > password; touch made"#,
        ),
    ];
    tools.extend(STANDARD);
    let mut script = ["stage_executable() { :; }\n", STEPS].concat();
    for (tool, body) in &tools {
        let path = dir.path().join(Path::new(tool).file_name().unwrap());
        fs::write(
            &path,
            format!("#!/bin/zsh -f\ncd {}\n{body}\n", dir.path().display()),
        )
        .unwrap();
        fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        script = script.replace(tool, &path.to_string_lossy());
    }
    let picture = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(PICTURE)
    };
    let full = "Atlas $x * helper";

    let (ours, theirs) = UnixStream::pair().unwrap();
    let mut child = atlas_core::spawn::spawn(
        Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args(["create", "opie", EXECUTABLE, "502", full, &picture])
            .stdin(std::os::fd::OwnedFd::from(theirs.try_clone().unwrap()))
            .stdout(std::os::fd::OwnedFd::from(theirs)),
    )
    .unwrap();
    let (finished, text) = converse(ours, Some(b"se cret$1"));
    child.wait().unwrap();
    let text = text.unwrap();

    assert!(finished);
    assert_eq!(outcome(&text), Ok(()), "{text}");
    assert!(text.contains("@atlas created 0"), "{text}");
    assert!(!text.contains("se cret$1"), "the password never comes back");
    assert_eq!(
        fs::read_to_string(dir.path().join("password")).unwrap(),
        "se cret$1"
    );
    let logged = fs::read_to_string(dir.path().join("sysadminctl.log")).unwrap();
    let arguments: Vec<&str> = logged.lines().collect();
    assert_eq!(
        arguments[..8],
        [
            "-addUser",
            "opie",
            "-fullName",
            full,
            "-UID",
            "502",
            "-password",
            "-"
        ]
    );
    assert_eq!(arguments[8], "-picture");
    assert_eq!(fs::read(dir.path().join("picture.png")).unwrap(), PICTURE);
}
