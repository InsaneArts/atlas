use super::*;
use serde_json::json;

#[test]
fn io_causes_keep_their_kind_and_errno() {
    for original in [
        std::io::Error::from_raw_os_error(libc::ENOENT),
        std::io::Error::new(std::io::ErrorKind::TimedOut, "Read timed out"),
    ] {
        let message = original.to_string();
        let kind = original.kind();
        let errno = original.raw_os_error();
        let error = Error::from(original);
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            json!({"ok": false, "code": "io", "error": message, "uncertain": false})
        );
        assert_eq!(error.to_string(), message);
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(source.kind(), kind);
        assert_eq!(source.raw_os_error(), errno);
    }
}

#[test]
fn json_causes_keep_their_location() {
    let original = serde_json::from_str::<serde_json::Value>("{\n  \"missing\":\n}").unwrap_err();
    let message = original.to_string();
    let location = (original.line(), original.column(), original.classify());
    let error = Error::from(original);
    assert_eq!(
        serde_json::to_value(&error).unwrap(),
        json!({"ok": false, "code": "invalid-json", "error": message, "uncertain": false})
    );
    assert_eq!(error.to_string(), message);
    let source = error
        .source()
        .unwrap()
        .downcast_ref::<serde_json::Error>()
        .unwrap();
    assert_eq!(
        (source.line(), source.column(), source.classify()),
        location
    );
}

#[test]
fn private_causes_do_not_appear_in_public_diagnostics() {
    #[derive(Debug)]
    struct PrivateCause(&'static str);

    impl fmt::Display for PrivateCause {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }

    impl StdError for PrivateCause {}

    let canary = "private-source-canary-7c81e9";
    let source = std::io::Error::other(PrivateCause(canary));
    let error = Error::new(Code::Io, "Read failed")
        .with_source(source)
        .uncertain()
        .held_by(Holder::Agent)
        .with("pending", json!(true));
    assert!(error.source().unwrap().to_string().contains(canary));
    for diagnostic in [
        error.to_string(),
        format!("{error:?}"),
        format!("{error:#?}"),
        serde_json::to_string(&error).unwrap(),
        serde_json::to_string_pretty(&error).unwrap(),
    ] {
        assert!(!diagnostic.contains(canary), "{diagnostic}");
        assert!(!diagnostic.contains("PrivateCause"), "{diagnostic}");
        assert!(diagnostic.contains("Read failed"), "{diagnostic}");
    }
    assert_eq!(
        serde_json::to_value(&error).unwrap(),
        json!({"ok": false, "code": "io", "error": "Read failed", "uncertain": true,
            "holder": "agent", "pending": true})
    );
}

#[test]
fn reserved_detail_keys_never_replace_failure_fields() {
    let mut error = Error::new(Code::Busy, "Wait")
        .uncertain()
        .held_by(Holder::Person)
        .with("detail", serde_json::Value::Null);
    let expected = serde_json::to_value(&error).unwrap();
    for key in ["ok", "code", "error", "uncertain", "holder"] {
        error = error.with(key, json!("replacement"));
        assert_eq!(serde_json::to_value(&error).unwrap(), expected);
        assert!(!error.details.contains_key(key));
    }
    assert_eq!(error.details.get("detail"), Some(&serde_json::Value::Null));
    let plain = Error::new(Code::Busy, "Wait").with("holder", json!("agent"));
    assert!(serde_json::to_value(plain).unwrap().get("holder").is_none());
}

#[test]
fn a_failure_names_its_holder_and_details_only_when_set() {
    let plain = Error::new(Code::Busy, "Wait");
    assert_eq!(
        serde_json::to_value(&plain).unwrap(),
        serde_json::json!({"ok": false, "code": "busy", "error": "Wait", "uncertain": false})
    );

    let held = Error::new(Code::Busy, "Wait")
        .held_by(Holder::Person)
        .with("next", serde_json::json!("Wait for them"));
    assert_eq!(
        serde_json::to_value(&held).unwrap(),
        serde_json::json!({"ok": false, "code": "busy", "error": "Wait", "uncertain": false,
                           "holder": "person", "next": "Wait for them"})
    );

    assert_eq!(Holder::parse("agent"), Some(Holder::Agent));
    assert_eq!(Holder::parse("person"), Some(Holder::Person));
    assert_eq!(Holder::parse("someone"), None);
}

/// Agents learn every code from the plugin's table, and only those.
#[test]
fn the_codes_are_the_documented_ones() {
    let table = include_str!("../../../agents/atlas/skills/atlas/references/troubleshooting.md");
    let documented: Vec<&str> = table
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split('`').next())
        .collect();

    assert_eq!(
        documented, CODES,
        "keep CODES and troubleshooting.md in step"
    );
    assert_eq!(Code::parse("busy"), Some(Code::Busy));
    assert_eq!(Code::parse("made-up"), None);
}

/// Each code is written as the same word everywhere, whichever way
/// it goes out: programs, Atlas.app and older builds match on these.
#[test]
fn every_code_keeps_its_word() {
    let words = [
        (Code::Agents, "agents"),
        (Code::Apps, "apps"),
        (Code::BadRequest, "bad-request"),
        (Code::Blocked, "blocked"),
        (Code::Busy, "busy"),
        (Code::Cancelled, "cancelled"),
        (Code::Capture, "capture"),
        (Code::Cleanup, "cleanup"),
        (Code::Collect, "collect"),
        (Code::Config, "config"),
        (Code::ConfigInvalid, "config-invalid"),
        (Code::Daemon, "daemon"),
        (Code::Elements, "elements"),
        (Code::Environment, "environment"),
        (Code::FocusChanged, "focus-changed"),
        (Code::Identity, "identity"),
        (Code::Init, "init"),
        (Code::Input, "input"),
        (Code::Install, "install"),
        (Code::Interrupted, "interrupted"),
        (Code::InvalidJson, "invalid-json"),
        (Code::Io, "io"),
        (Code::Job, "job"),
        (Code::LeaseLost, "lease-lost"),
        (Code::LoggedOut, "logged-out"),
        (Code::Logout, "logout"),
        (Code::LogoutHeld, "logout-held"),
        (Code::Menubar, "menubar"),
        (Code::Native, "native"),
        (Code::NoEnvironment, "no-environment"),
        (Code::NoWindow, "no-window"),
        (Code::NotReady, "not-ready"),
        (Code::OnConsole, "on-console"),
        (Code::Onboarding, "onboarding"),
        (Code::Permission, "permission"),
        (Code::Platform, "platform"),
        (Code::PortInUse, "port-in-use"),
        (Code::Protocol, "protocol"),
        (Code::Restart, "restart"),
        (Code::SessionChanged, "session-changed"),
        (Code::Setup, "setup"),
        (Code::SpawnFailed, "spawn-failed"),
        (Code::StaleFrame, "stale-frame"),
        (Code::Startup, "startup"),
        (Code::State, "state"),
        (Code::Stopped, "stopped"),
        (Code::Switch, "switch"),
        (Code::Terminal, "terminal"),
        (Code::Transport, "transport"),
        (Code::Uncertain, "uncertain"),
        (Code::Uninstall, "uninstall"),
        (Code::Update, "update"),
        (Code::Usage, "usage"),
        (Code::WaitFailed, "wait-failed"),
        (Code::Windows, "windows"),
    ];
    assert_eq!(words.map(|(code, _)| code), Code::ALL);
    for (code, word) in words {
        assert_eq!(code.as_str(), word);
        assert_eq!(serde_json::to_value(code).unwrap(), word);
        assert_eq!(serde_json::from_value::<Code>(word.into()).unwrap(), code);
        assert_eq!(code.to_string(), word);
    }
    assert_eq!(CODES, words.map(|(_, word)| word));
}
