use super::{recovery::owns_every_unknown, *};
use crate::control::Cause;
use std::{cell::RefCell, collections::HashMap};

const CLAUDE_ID: &str = "d9031471-5e2a-4c8b-9f1d-2b7c3a6e0f44";

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

fn ancestor(pid: i32, command: &str) -> Ancestor {
    Ancestor {
        pid,
        uid: nix::unistd::geteuid().as_raw(),
        parent: 1,
        instance: Instance::new(pid, [1_761_655_000, 412_233]),
        command: command.to_owned(),
    }
}

fn resolved(pairs: &[(&str, &str)], ancestors: Vec<Ancestor>) -> Option<HostSession> {
    HostSession::resolve(
        env(pairs),
        || ancestors.clone(),
        |pid| {
            ancestors
                .iter()
                .find(|ancestor| ancestor.pid == pid)
                .map(|ancestor| Facts {
                    uid: nix::unistd::geteuid().as_raw(),
                    parent: ancestor.parent,
                    command: ancestor.command.clone(),
                    start: ancestor.instance.start,
                })
        },
    )
}

#[test]
fn a_claude_code_command_is_its_session_and_names_its_process() {
    let session = resolved(
        &[
            ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
            ("CLAUDE_PID", "4242"),
        ],
        vec![ancestor(4242, "claude")],
    )
    .unwrap();
    let hex = hex(&Sha256::digest(format!("claude:{CLAUDE_ID}").as_bytes()));

    assert_eq!(session.host, Host::Claude);
    assert_eq!(session.key.as_str(), &hex[..KEY_LEN]);
    assert_eq!(session.id.as_str(), CLAUDE_ID);
    assert_eq!(
        session.process,
        Some(Instance::new(4242, [1_761_655_000, 412_233]))
    );
}

#[test]
fn a_claude_pid_that_is_not_running_is_left_out() {
    let session = HostSession::resolve(
        env(&[
            ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
            ("CLAUDE_PID", "4242"),
        ]),
        Vec::new,
        |_| None,
    )
    .unwrap();

    assert_eq!(session.key, Key::of(Host::Claude, CLAUDE_ID));
    assert_eq!(session.process, None);
}

#[test]
fn codex_prefers_the_root_session_id_to_the_thread_id() {
    let both = resolved(
        &[("CODEX_SESSION_ID", "root"), ("CODEX_THREAD_ID", "thread")],
        Vec::new(),
    )
    .unwrap();
    assert_eq!(both.key, Key::of(Host::Codex, "root"));

    let thread = resolved(&[("CODEX_THREAD_ID", "thread")], Vec::new()).unwrap();
    assert_eq!(thread.key, Key::of(Host::Codex, "thread"));
    assert_eq!(thread.process, None, "no verified ancestor");
}

#[test]
fn a_codex_subagent_and_its_parent_are_one_session() {
    let parent = resolved(
        &[("CODEX_SESSION_ID", "root"), ("CODEX_THREAD_ID", "root")],
        Vec::new(),
    );
    let subagent = resolved(
        &[("CODEX_SESSION_ID", "root"), ("CODEX_THREAD_ID", "child")],
        Vec::new(),
    );

    assert_eq!(parent.unwrap().key, subagent.unwrap().key);
}

#[test]
fn pi_and_opencode_are_read_from_their_own_variables() {
    let pi = resolved(&[("PI_SESSION_ID", "pi-1")], Vec::new()).unwrap();
    assert_eq!((pi.host, pi.id.as_str()), (Host::Pi, "pi-1"));

    let opencode = resolved(&[("ATLAS_OPENCODE_SESSION", "ses_7f3a")], Vec::new()).unwrap();
    assert_eq!(
        (opencode.host, opencode.id.as_str()),
        (Host::Opencode, "ses_7f3a")
    );
}

#[test]
fn a_named_session_is_used_only_without_a_host() {
    let named = resolved(&[("ATLAS_SESSION", "release")], Vec::new()).unwrap();
    assert_eq!((named.host, named.id.as_str()), (Host::Named, "release"));

    let hosted = resolved(
        &[("ATLAS_SESSION", "release"), ("CODEX_THREAD_ID", "thread")],
        Vec::new(),
    )
    .unwrap();
    assert_eq!(hosted.host, Host::Codex);
}

#[test]
fn a_value_that_is_not_a_plain_id_is_no_session() {
    let long = "a".repeat(ID_MAX + 1);
    for bad in [long.as_str(), "two words", "a/b", "nul\0", "", "café"] {
        assert_eq!(
            resolved(&[("PI_SESSION_ID", bad)], Vec::new()),
            None,
            "{bad:?}"
        );
        let next = resolved(
            &[("CODEX_SESSION_ID", bad), ("CODEX_THREAD_ID", "thread")],
            Vec::new(),
        )
        .unwrap();
        assert_eq!(next.key, Key::of(Host::Codex, "thread"), "{bad:?}");
    }
    assert!(plain(&"a".repeat(ID_MAX)));
}

#[test]
fn the_nearest_host_wins_when_hosts_are_nested() {
    let pairs = [
        ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
        ("CLAUDE_PID", "4242"),
        ("CODEX_THREAD_ID", "thread"),
    ];
    let codex_inside = vec![
        ancestor(90, "zsh"),
        ancestor(80, "codex"),
        ancestor(70, "zsh"),
        ancestor(4242, "claude"),
    ];
    let codex = resolved(&pairs, codex_inside).unwrap();
    assert_eq!(codex.host, Host::Codex);
    assert_eq!(codex.process, Some(ancestor(80, "codex").instance));

    let claude_inside = vec![
        ancestor(90, "zsh"),
        ancestor(4242, "claude"),
        ancestor(70, "zsh"),
        ancestor(60, "codex"),
    ];
    let claude = resolved(&pairs, claude_inside).unwrap();
    assert_eq!(claude.host, Host::Claude);
    assert!(claude.process.is_some());
}

#[test]
fn nested_hosts_the_walk_cannot_place_name_no_session() {
    let pairs = [
        ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
        ("CLAUDE_PID", "4242"),
        ("CODEX_THREAD_ID", "thread"),
    ];
    let neither = vec![ancestor(90, "zsh"), ancestor(1, "launchd")];
    assert_eq!(resolved(&pairs, neither), None);

    // Pi runs as node: Claude Code is found, but not Pi inside it.
    let pi_in_claude = [
        ("CLAUDE_CODE_SESSION_ID", CLAUDE_ID),
        ("CLAUDE_PID", "4242"),
        ("PI_SESSION_ID", "pi-1"),
    ];
    let walk = vec![
        ancestor(90, "zsh"),
        ancestor(80, "node"),
        ancestor(70, "zsh"),
        ancestor(4242, "claude"),
    ];
    assert_eq!(resolved(&pi_in_claude, walk), None);

    // A walk cut short (by its limit, or a process it could not read)
    // before the outer host.
    let cut_short = vec![ancestor(90, "zsh"), ancestor(80, "codex")];
    assert_eq!(resolved(&pairs, cut_short), None);
}

#[test]
fn ancestry_is_asked_for_a_single_host_too() {
    let asked = RefCell::new(0);
    let ancestors = || {
        *asked.borrow_mut() += 1;
        Vec::new()
    };
    HostSession::resolve(env(&[("PI_SESSION_ID", "pi-1")]), ancestors, |_| None).unwrap();
    assert_eq!(*asked.borrow(), 1);
}

#[test]
fn nothing_reserved_or_private_is_read() {
    let read = RefCell::new(Vec::new());
    let counting = |name: &str| {
        read.borrow_mut().push(name.to_owned());
        None
    };
    assert_eq!(HostSession::resolve(counting, Vec::new, |_| None), None);

    let read = read.into_inner();
    for never in [
        "PI_SESSION_FILE",
        "CLAUDE_CODE_BRIDGE_SESSION_ID",
        "CODEX_TOOL_CALL_ID",
        "CURSOR_TRANSCRIPT_PATH",
        "AI_AGENT",
    ] {
        assert!(!read.iter().any(|name| name == never), "{never}");
    }
    assert!(
        read.iter()
            .all(|name| !name.contains("TRANSCRIPT") && !name.contains("COMPANION")),
        "{read:?}"
    );
}

#[test]
fn a_session_owns_the_unknown_only_when_every_one_is_its_own_and_none_runs() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>, running: bool| Cause {
        generation: crate::files::token(),
        running,
        command: false,
        session: session.cloned(),
    };

    assert!(!owns_every_unknown(&[], &own), "nothing is unknown");
    assert!(owns_every_unknown(&[cause(Some(&own), false)], &own));
    assert!(!owns_every_unknown(&[cause(Some(&own), true)], &own));
    assert!(!owns_every_unknown(
        &[cause(Some(&own), false), cause(Some(&other), false)],
        &own
    ));
    assert!(!owns_every_unknown(
        &[cause(Some(&own), false), cause(None, false)],
        &own
    ));
}

#[test]
fn a_label_names_the_folder_and_only_a_named_sessions_own_label() {
    let claude = HostSession::new(Host::Claude, CLAUDE_ID)
        .unwrap()
        .in_folder(Some("left"));
    let label = claude.label();
    assert_eq!(label.folder.as_deref(), Some("left"));
    assert_eq!(label.named, None, "a host's own id stays private");

    let named = HostSession::new(Host::Named, "release").unwrap().label();
    assert_eq!(named.named.as_ref().map(HostId::as_str), Some("release"));
    assert_eq!(named.folder, None);
}

#[test]
fn a_running_command_is_nobodys_unknown_outcome_and_recovery_waits_for_it() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let unknown = Cause {
        generation: crate::files::token(),
        running: false,
        command: false,
        session: Some(own.clone()),
    };
    let command = Cause {
        generation: crate::files::token(),
        running: true,
        command: true,
        session: Some(other),
    };
    let of = Recovery::of;
    let beside = [unknown.clone(), command.clone()];

    assert!(owns_every_unknown(&beside, &own));
    assert_eq!(
        of(&beside, Asker::Agent(Some(&own))),
        Some(Recovery::OwnOnceCommandEnds)
    );
    assert_eq!(
        of(&beside, Asker::Person),
        Some(Recovery::PersonOnceCommandEnds)
    );
    assert_eq!(
        of(&[unknown], Asker::Agent(Some(&own))),
        Some(Recovery::Own)
    );
    assert_eq!(
        of(std::slice::from_ref(&command), Asker::Person),
        None,
        "work under way"
    );
    assert!(!owns_every_unknown(&[command], &own), "nothing of its own");
    for step in [
        Recovery::OwnOnceCommandEnds.step(),
        Recovery::PersonOnceCommandEnds.step(),
    ] {
        assert!(step.contains("once the running command ends"), "{step}");
    }
    assert!(Recovery::OwnOnceCommandEnds.own());
}

/// The guide tells an agent whose command its host stopped to run
/// doctor and restart only when it says every unknown action is its
/// own. Doctor's step is [`Recovery::step`]: it names a restart only for
/// a session that owns every unknown change, never for mixed sessions'
/// changes or one whose session is not known, and a command stopped
/// while it only waited left nothing to recover.
#[test]
fn the_guide_restarts_after_a_stopped_command_only_when_recovery_says_so() {
    let core = crate::agent::CORE
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(core.contains(
        "If your host stopped a command of yours, run `atlas doctor`; restart only when it says all is yours."
    ));

    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>| Cause {
        generation: crate::files::token(),
        running: false,
        command: false,
        session: session.cloned(),
    };
    let asker = Asker::Agent(Some(&own));
    assert_eq!(Recovery::of(&[], asker), None, "only waiting");
    for (causes, yours) in [
        (vec![cause(Some(&own))], true),
        (vec![cause(Some(&own)), cause(Some(&other))], false),
        (vec![cause(Some(&own)), cause(None)], false),
        (vec![cause(Some(&other))], false),
    ] {
        let recovery = Recovery::of(&causes, asker).unwrap();
        assert_eq!(recovery.own(), yours, "{recovery:?}");
        assert_eq!(
            recovery.step().contains("atlas restart"),
            yours,
            "{}",
            recovery.step()
        );
    }
}

#[test]
fn only_the_session_that_claimed_every_unknown_change_recovers_it_itself() {
    let (own, other) = (Key::of(Host::Pi, "own"), Key::of(Host::Pi, "other"));
    let cause = |session: Option<&Key>, running: bool| Cause {
        generation: crate::files::token(),
        running,
        command: false,
        session: session.cloned(),
    };
    let of = Recovery::of;

    assert_eq!(
        of(&[], Asker::Agent(Some(&own))),
        None,
        "nothing is pending"
    );
    assert_eq!(
        of(&[cause(Some(&own), true)], Asker::Person),
        None,
        "a change still being carried out is work in progress"
    );
    let mine = [cause(Some(&own), false)];
    assert_eq!(of(&mine, Asker::Agent(Some(&own))), Some(Recovery::Own));
    assert_eq!(
        of(&mine, Asker::Agent(Some(&other))),
        Some(Recovery::Others)
    );
    assert_eq!(of(&mine, Asker::Agent(None)), Some(Recovery::Others));
    assert_eq!(of(&mine, Asker::Person), Some(Recovery::Person));
    for shared in [
        [cause(Some(&own), false), cause(Some(&other), false)],
        [cause(Some(&own), false), cause(None, false)],
        [cause(Some(&own), false), cause(Some(&own), true)],
    ] {
        assert_eq!(
            of(&shared, Asker::Agent(Some(&own))),
            Some(Recovery::Others)
        );
    }

    assert!(Recovery::Own.step().contains("atlas restart"));
    assert!(Recovery::Person.step().contains("atlas restart"));
    let others = Recovery::Others.step();
    assert!(
        !others.contains("atlas "),
        "no command for another's action"
    );
    assert!(others.contains("tell the person"), "{others}");
}

#[test]
fn a_key_is_twelve_lowercase_hex_digits() {
    let key = Key::of(Host::Pi, "pi-1");
    assert_eq!(Key::parse(key.as_str()), Some(key.clone()));
    for bad in [
        "3F9A1C07B2E4",
        "3f9a1c07b2e",
        "3f9a1c07b2e4f",
        "3f9a1c07b2eg",
    ] {
        assert_eq!(Key::parse(bad), None, "{bad}");
    }
    let through: Key = serde_json::from_value(serde_json::json!(key.as_str())).unwrap();
    assert_eq!(through, key);
    assert!(serde_json::from_value::<Key>(serde_json::json!("../x")).is_err());
}

#[test]
fn every_unambiguously_identified_host_keeps_its_verified_process_instance() {
    for (source, command) in [
        ("CODEX_SESSION_ID", "codex"),
        ("PI_SESSION_ID", "pi"),
        ("ATLAS_OPENCODE_SESSION", "opencode"),
    ] {
        let root = ancestor(80, command);
        let session =
            resolved(&[(source, "root")], vec![ancestor(90, "sh"), root.clone()]).unwrap();
        assert_eq!(session.process(), Some(root.instance));
    }
}

#[test]
fn a_node_host_hint_must_name_an_ancestor_of_this_account() {
    for (source, hint) in [
        ("PI_SESSION_ID", "ATLAS_PI_PID"),
        ("ATLAS_OPENCODE_SESSION", "ATLAS_OPENCODE_PID"),
    ] {
        for pid in ["80", "81", "bad", "-1"] {
            let session =
                resolved(&[(source, "root"), (hint, pid)], vec![ancestor(80, "node")]).unwrap();
            assert_eq!(session.process().is_some(), pid == "80");
        }
        let session = HostSession::resolve(
            env(&[(source, "root"), (hint, "80")]),
            || vec![ancestor(80, "node")],
            |_| None,
        )
        .unwrap();
        assert_eq!(session.process(), None);
        let session = HostSession::resolve(
            env(&[(source, "root"), (hint, "80")]),
            || vec![ancestor(80, "node")],
            |_| {
                Some(Facts {
                    uid: nix::unistd::geteuid().as_raw(),
                    parent: 1,
                    command: "node".into(),
                    start: [2, 0],
                })
            },
        )
        .unwrap();
        assert_eq!(session.process(), None);
    }
}

#[test]
fn a_root_codex_id_and_its_subagent_share_the_sole_verified_host() {
    let walk = vec![ancestor(80, "codex")];
    let root = resolved(
        &[("CODEX_SESSION_ID", "root"), ("CODEX_THREAD_ID", "root")],
        walk.clone(),
    )
    .unwrap();
    let child = resolved(
        &[("CODEX_SESSION_ID", "root"), ("CODEX_THREAD_ID", "child")],
        walk,
    )
    .unwrap();
    assert_eq!(root.key(), child.key());
    assert_eq!(root.process(), child.process());
    assert!(root.process().is_some());
}

#[test]
fn independently_nested_codex_hosts_have_no_guessed_process_or_folder() {
    let session = resolved(
        &[("CODEX_SESSION_ID", "root")],
        vec![ancestor(80, "codex"), ancestor(70, "codex")],
    )
    .unwrap()
    .with_folder(|_| panic!("no selected host"), Some("/tmp/child".into()));
    assert_eq!(session.process(), None);
    assert_eq!(session.folder(), None);
}

#[test]
fn an_incomplete_codex_walk_cannot_choose_a_root() {
    let mut root = ancestor(80, "codex");
    root.parent = 70;
    let session = resolved(&[("CODEX_SESSION_ID", "root")], vec![root]).unwrap();
    assert_eq!(session.process(), None);
}

#[test]
fn a_subagent_uses_its_root_hosts_folder() {
    let session = resolved(&[("CODEX_SESSION_ID", "root")], vec![ancestor(80, "codex")])
        .unwrap()
        .with_folder(|_| Some("/tmp/parent".into()), Some("/tmp/child".into()));
    assert_eq!(session.folder(), Some("parent"));
    assert!(session.folder_from_host);
}

#[test]
fn a_failed_host_folder_read_never_uses_a_childs_folder() {
    let session = resolved(&[("CODEX_SESSION_ID", "root")], vec![ancestor(80, "codex")])
        .unwrap()
        .with_folder(|_| None, Some("/tmp/child".into()));
    assert_eq!(session.folder(), None);
    assert!(!session.folder_from_host);
}
