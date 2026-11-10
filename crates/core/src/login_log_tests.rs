use super::*;

fn at(seconds: f64, source: Source, pid: u64, message: &str) -> Event {
    Event {
        at: 1_000.0 + seconds,
        source,
        pid,
        message: message.to_owned(),
    }
}

#[test]
fn a_log_line_is_read_with_its_time_zone() {
    let line = r#"{"subsystem":"com.apple.loginwindow.logging","processImagePath":"\/System\/Library\/CoreServices\/loginwindow.app\/Contents\/MacOS\/loginwindow","processID":31095,"timestamp":"2026-10-23 20:50:04.250000+0400","eventMessage":"-[SessionStateMonitor setLoginState:] | ************* login state: LoginComplete"}"#;
    let read = event(line).unwrap();
    assert_eq!(read.source, Source::LoginWindow);
    assert_eq!(read.pid, 31095);
    // 2026-10-23 16:50:04.25 UTC.
    assert!((read.at - 1_792_774_204.25).abs() < 1e-6, "{}", read.at);

    let west = timestamp("2026-10-23 20:50:04.250000-0400").unwrap();
    assert!((west - read.at - 8.0 * 3600.0).abs() < 1e-6);
    assert_eq!(timestamp("yesterday"), None);

    let atlas = r#"{"subsystem":"com.insanearts.atlas","processImagePath":"\/Applications\/Atlas.app\/Contents\/MacOS\/atlas","timestamp":"2026-10-23 20:50:05.000000+0400","eventMessage":"opener.start"}"#;
    assert_eq!(event(atlas).unwrap().source, Source::Atlas);
    assert!(event("not json").is_none());
}

#[test]
fn the_last_login_is_told_step_by_step() {
    use Source::{Atlas, LoginWindow, Updater};

    let events = [
        at(-600.0, LoginWindow, 1, "login state: ReturnFromAuthCopy"),
        at(
            0.0,
            LoginWindow,
            7,
            "-[SessionStateMonitor setLoginState:] | ************* login state: ReturnFromAuthCopy",
        ),
        at(0.1, LoginWindow, 7, "login state: CreateAndSwitch"),
        at(0.5, Updater, 40, "UAU Session: -(-)->27.0.1, UID: 503"),
        at(
            0.6,
            Updater,
            40,
            "UAU: Plugin com.apple.dock.DockUpdater loaded.",
        ),
        at(
            0.6,
            Updater,
            40,
            "UAU: Plugin com.apple.dock.SpacesUpdater loaded.",
        ),
        at(
            0.6,
            Updater,
            40,
            "UAU: Plugin com.apple.Safari.UserAccountUpdater opts out of this session.",
        ),
        at(
            0.7,
            Updater,
            40,
            "UAU Plugin com.apple.dock.DockUpdater Root phase completed in 0.000015 seconds.",
        ),
        at(
            0.8,
            Updater,
            40,
            "UAU Plugin com.apple.dock.DockUpdater User phase completed in 0.040805 seconds.",
        ),
        at(
            0.9,
            Updater,
            40,
            "UAU Plugin com.apple.dock.SpacesUpdater Root phase completed in 0.300000 seconds.",
        ),
        at(
            3.0,
            LoginWindow,
            7,
            "-[SessionStateMonitor setLoginState:] | ************* login state: LoginComplete",
        ),
        at(
            4.0,
            Atlas,
            50,
            "opener.desktop waited=3.2s setupAssistant=false",
        ),
        at(6.5, Atlas, 50, "opener.window waited=2.5s"),
    ];
    let summary = summarize(&events);

    assert_eq!(summary["found"], true);
    assert_eq!(summary["steps"]["passwordToUpdater"], 0.5);
    assert_eq!(
        summary["steps"]["updater"],
        Value::Null,
        "an update with a plugin left unfinished never ended"
    );
    assert_eq!(summary["steps"]["setupAssistant"], false);
    assert_eq!(summary["steps"]["loginCompleteToDesktop"], 1.0);
    assert_eq!(summary["steps"]["desktopToAtlas"], 2.5);
    assert_eq!(summary["steps"]["passwordToDesktop"], 4.0);
    assert_eq!(
        summary["slowestPlugin"],
        json!({"plugin": "com.apple.dock.SpacesUpdater", "phase": "Root", "seconds": 0.3})
    );
    assert_eq!(
        summary["unfinished"],
        json!(["com.apple.dock.SpacesUpdater"]),
        "its user phase never finished"
    );
    assert_eq!(summary["markers"][1]["after"], 6.5);
}

#[test]
fn no_login_is_said_plainly_and_missing_steps_are_unknown() {
    use Source::LoginWindow;

    assert_eq!(summarize(&[])["found"], false);

    let unlock = [at(0.0, LoginWindow, 7, "login state: ReturnFromAuthCopy")];
    assert_eq!(
        summarize(&unlock)["found"],
        false,
        "a screen unlocked is not a login"
    );

    let hung = [
        at(0.0, LoginWindow, 7, "login state: ReturnFromAuthCopy"),
        at(0.1, LoginWindow, 7, "login state: CreateAndSwitch"),
        at(0.2, LoginWindow, 7, "login state: UAULaunched"),
    ];
    let summary = summarize(&hung);
    assert_eq!(summary["found"], true);
    assert_eq!(summary["steps"]["passwordToDesktop"], Value::Null);
    assert_eq!(summary["unfinished"], json!([]));
}

#[test]
fn the_login_after_atlas_locked_the_screen_is_the_one_told() {
    use Source::{Atlas, LoginWindow};

    let events = [
        at(0.0, Atlas, 1, "switch.locked helper=503"),
        at(5.0, LoginWindow, 7, "login state: ReturnFromAuthCopy"),
        at(5.1, LoginWindow, 7, "login state: CreateAndSwitch"),
        at(90.0, LoginWindow, 8, "login state: ReturnFromAuthCopy"),
        at(90.1, LoginWindow, 8, "login state: CreateAndSwitch"),
        at(
            95.0,
            Atlas,
            2,
            "opener.desktop waited=1.0s setupAssistant=true",
        ),
    ];
    let summary = summarize(&events);
    assert_eq!(summary["steps"]["passwordToDesktop"], 90.0);
    assert_eq!(summary["steps"]["setupAssistant"], true);
}
