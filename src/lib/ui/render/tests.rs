//! Each view against the JSON its command returns (shapes taken from real
//! helper replies), with colour off.
use super::*;
use serde_json::json;

fn show(view: View, value: Value) -> Rendered {
    console::set_colors_enabled(false);
    console::set_colors_enabled_stderr(false);
    render(&view, &value)
}

#[test]
fn an_opencode_change_names_its_own_restart_without_a_claude_instruction() {
    for connected in [true, false] {
        let shown = show(
            View::AgentsChange,
            json!({"ok":true,"agents":[{
                "agent":"opencode","ok":true,"connected":connected,"restart":true
            }]}),
        );
        assert!(
            shown
                .out
                .contains("Restart opencode for this change to take effect.")
        );
        assert!(!shown.out.contains("Claude app"));
    }
}

#[test]
fn a_cursor_change_names_its_reload_without_a_claude_instruction() {
    for connected in [true, false] {
        let shown = show(
            View::AgentsChange,
            json!({"ok":true,"agents":[{
                "agent":"cursor","ok":true,"connected":connected,"restart":true
            }]}),
        );
        assert!(
            shown
                .out
                .contains("Reload the Cursor window or start a new Cursor Agent session")
        );
        assert!(!shown.out.contains("Claude app"));
    }
}

/// Errors, steps and warnings can carry the helper's text: it never drives
/// the terminal, while the lines Atlas wrote stay lines.
#[test]
fn helper_text_in_errors_and_steps_is_made_safe() {
    let shown = show(
        View::Generic,
        json!({"ok": false, "error": "Bad \u{1b}[2J name\nsecond line"}),
    );
    assert!(
        shown.err.starts_with("✗ Bad \\u{1b}[2J name\nsecond line"),
        "{}",
        shown.err
    );

    let report = json!({
        "ok": true, "ready": false, "checks": [],
        "nextSteps": ["Fix \u{1b}]0;title\u{7}"],
        "warnings": ["Odd \u{202e}name"],
    });
    let out = show(View::Doctor { verbose: false }, report).out;
    assert!(
        !out.contains('\u{1b}') && !out.contains('\u{202e}'),
        "{out}"
    );
}

#[test]
fn a_ready_doctor_is_one_line() {
    let report = json!({
        "ok": true, "ready": true,
        "checks": [{"name": "Helper running", "ok": true}],
        "nextSteps": [], "warnings": [],
        "helper": {"name": "atlas", "uid": 502},
        "display": {"width": 2560.0, "height": 1440.0, "scale": 1.0},
    });

    assert_eq!(
        show(View::Doctor { verbose: false }, report).out,
        "✓ Atlas is ready  helper atlas · 2560 × 1440"
    );
}

#[test]
fn a_doctor_that_is_not_ready_lists_checks_and_fixes() {
    let report = json!({
        "ok": true, "ready": false,
        "checks": [
            {"name": "Helper running", "ok": true},
            {"name": "Helper in the background", "ok": false},
            {"name": "Input service", "ok": null},
        ],
        "nextSteps": ["Switch back to your own account."],
        "warnings": ["The input service is from an older build."],
    });

    assert_eq!(
        show(View::Doctor { verbose: false }, report).out,
        "✗ Atlas is not ready\n\n  ✓ Helper running\n  ✗ Helper in the background\n  – Input service\n\nTo fix:\n  1. Switch back to your own account.\n! The input service is from an older build."
    );
}

#[test]
fn errors_say_what_happened_and_what_to_do() {
    console::set_colors_enabled(false);

    let plain = Error::new(Code::OnConsole, "The helper is on screen right now");
    assert_eq!(
        error(&plain),
        "✗ The helper is on screen right now\n  → Switch back to your own account. Atlas works while the helper is in the background."
    );

    let says = Error::new(Code::Config, "No configuration; run atlas init");
    assert_eq!(error(&says), "✗ No configuration; run atlas init");

    let unsure = Error::new(Code::Transport, "The helper stopped answering").uncertain();
    assert!(error(&unsure).contains("don't repeat it"));
}

#[test]
fn a_disabled_control_keeps_its_next_step_in_human_output() {
    console::set_colors_enabled(false);

    let refused = Error::new(
        Code::Input,
        "The selected control is disabled; no click was sent",
    )
    .with("next", hints::DISABLED_CONTROL);
    let expected = format!("✗ {}\n  → {}", refused.error, hints::DISABLED_CONTROL);

    assert_eq!(error(&refused), expected);
    assert_eq!(show(View::Generic, json!(refused)), Rendered::err(expected));
}

#[test]
fn a_screenshot_names_its_file_and_size() {
    let shown = show(
        View::Capture,
        json!({"ok": true, "path": "/tmp/x.png", "width": 2560, "height": 1440, "pointsPerPixel": 1.0}),
    );
    assert_eq!(
        shown.out,
        "✓ Screenshot saved  /tmp/x.png\n  2560 × 1440 pixels · 1 point per pixel"
    );
}

#[test]
fn an_unchanged_screenshot_names_the_earlier_file() {
    let shown = show(
        View::Capture,
        json!({
            "ok": true, "unchanged": true, "path": "/tmp/1-capture.png", "width": 1456,
            "height": 819, "origin": {"x": 0.0, "y": 0.0}, "pointsPerPixel": 1.758,
        }),
    );
    assert_eq!(
        shown.out,
        "✓ Screenshot unchanged  /tmp/1-capture.png\n  1456 × 819 pixels · 1.8 points per pixel"
    );
}

#[test]
fn a_cropped_screenshot_names_its_origin_and_what_is_in_front() {
    let shown = show(
        View::Capture,
        json!({
            "ok": true, "path": "/tmp/x.png", "width": 800, "height": 600,
            "origin": {"x": 100.0, "y": 80.0}, "pointsPerPixel": 0.5,
            "region": {"x": 100, "y": 80, "width": 400, "height": 300},
            "app": {"pid": 42, "name": "TextEdit"},
            "overlapping": [
                {"owner": "System Events", "pid": 7, "title": null, "bounds": {}},
                {"owner": "System Events", "pid": 7, "title": "Two", "bounds": {}},
                {"owner": "Bad\u{1b}[2J", "pid": 8, "title": null, "bounds": {}},
            ],
        }),
    );
    assert_eq!(
        shown.out,
        "✓ Screenshot saved  /tmp/x.png\n  800 × 600 pixels · 0.5 points per pixel · origin 100, 80\n  In front of it: System Events, Bad\\u{1b}[2J"
    );

    let corner = show(
        View::Capture,
        json!({"ok": true, "path": "/tmp/x.png", "width": 2560, "height": 1440, "origin": {"x": 0.0, "y": 0.0}, "pointsPerPixel": 1.0, "overlapping": []}),
    );
    assert_eq!(
        corner.out,
        "✓ Screenshot saved  /tmp/x.png\n  2560 × 1440 pixels · 1 point per pixel"
    );
}

#[test]
fn run_prints_the_output_and_notes_a_failure_apart() {
    let ok = show(
        View::Run,
        json!({"ok": true, "output": "hi\n", "exitCode": 0, "signal": null, "timedOut": false}),
    );
    assert_eq!(ok, Rendered::out("hi\n"));

    let failed = show(
        View::Run,
        json!({"ok": true, "output": "", "exitCode": 1, "signal": null, "timedOut": false, "durationMs": 12}),
    );
    assert_eq!(failed.err, "✗ Exited with 1 · 12 ms");
}

#[test]
fn windows_are_a_table_with_names_made_safe() {
    let shown = show(
        View::Windows,
        json!({"ok": true, "windows": [{
            "owner": "TextEdit", "title": "Bad\u{1b}[2J", "pid": 812,
            "bounds": {"x": 120.0, "y": 80.0, "width": 900.0, "height": 640.0},
        }]}),
    );
    assert!(shown.out.contains("Bad\\u{1b}[2J"));
    assert!(shown.out.contains("120, 80"));
    assert!(shown.out.ends_with("1 window, front to back"));
}

#[test]
fn apps_say_how_many_background_processes_were_left_out() {
    let shown = show(
        View::Apps,
        json!({"ok": true, "more": 1, "apps": [
            {"name": "TextEdit", "pid": 1, "policy": "regular", "active": true, "hidden": false},
        ]}),
    );
    assert!(shown.out.contains("TextEdit  1    frontmost"));
    assert!(
        shown
            .out
            .ends_with("1 background process not shown; --all lists them.")
    );

    let every = show(
        View::Apps,
        json!({"ok": true, "more": 0, "apps": [
            {"name": "TextEdit", "pid": 1, "policy": "regular", "active": true, "hidden": false},
            {"name": "loginwindow", "pid": 2, "policy": "accessory", "active": false, "hidden": false},
        ]}),
    );
    assert!(every.out.contains("loginwindow"));
    assert!(!every.out.contains("not shown"));
}

#[test]
fn a_failed_action_says_how_far_it_got() {
    let shown = show(
        View::Action("Pressed cmd+s".into()),
        json!({"ok": false, "code": "input", "error": "Unknown key", "performed": 1}),
    );
    assert!(shown.err.starts_with("✗ Unknown key"));
    assert!(shown.err.ends_with("1 action finished first."));

    let clicked = show(
        View::Action("Clicked".into()),
        json!({"ok": true, "performed": 1, "element": {"role": "AXButton", "title": "Save"}}),
    );
    assert_eq!(clicked.out, "✓ Clicked \"Save\" (button)");

    let named = show(
        View::Action("Clicked".into()),
        json!({"ok": true, "performed": 1, "element": {"at": [25, 25], "name": "Save", "role": "AXButton"}}),
    );
    assert_eq!(named.out, "✓ Clicked \"Save\" (button)");
}

#[test]
fn agents_are_a_table_with_a_way_to_connect() {
    let shown = show(
        View::Agents,
        json!({"ok": true, "agents": [
            {"agent": "claude", "installed": true, "version": "2.1.296 (Claude Code)", "connected": true},
            {"agent": "codex", "installed": true, "version": "codex-cli 0.162.0", "connected": false},
        ]}),
    );

    let stuck = show(
        View::Agents,
        json!({"ok": true, "agents": [
            {"agent": "codex", "installed": true, "error": "Codex did not answer within 10 seconds"},
        ]}),
    );

    assert!(
        stuck.out.contains("Codex") && stuck.out.contains("! not answering"),
        "{}",
        stuck.out
    );
    assert!(shown.out.contains("Claude Code  2.1.296  ✓ connected"));
    assert!(shown.out.contains("Codex        0.162.0  – not connected"));
    assert!(shown.out.ends_with("To connect: atlas agents install"));
}

#[test]
fn an_update_is_one_line_or_says_where_it_stopped() {
    let current = show(
        View::Update,
        json!({"ok": true, "updated": false, "version": "0.1.0"}),
    );
    assert_eq!(current.out, "✓ Atlas 0.1.0 is up to date");

    let stopped = show(
        View::Update,
        json!({"ok": false, "updated": true, "to": "Atlas-0.3.0-macos.zip",
               "helper": {"ok": false, "failedStep": "restart", "failure": {"error": "No answer"}}}),
    );
    assert!(
        stopped
            .err
            .contains("but restarting the helper failed: No answer")
    );
}

#[test]
fn results_without_a_view_are_summarised_not_dumped() {
    let shown = show(
        View::Generic,
        json!({"ok": true, "planId": "abc", "nested": {"a": 1}}),
    );
    assert_eq!(
        shown.out,
        "✓ Done\n  Plan id  abc\n  --json shows everything."
    );

    let failed = show(
        View::Generic,
        json!({"ok": false, "error": "No", "code": "setup"}),
    );
    assert!(failed.err.starts_with("✗ No"));
}

#[test]
fn an_update_before_setup_says_to_set_up() {
    let shown = show(
        View::Update,
        json!({"ok": true, "updated": true, "to": "Atlas-0.3.0-macos.zip",
               "helper": {"ok": true, "next": "atlas init"}}),
    );
    assert_eq!(
        shown.out,
        "✓ Updated Atlas to 0.3.0\n  Run atlas init to set up the helper."
    );
}

#[test]
fn a_verbose_doctor_shows_the_facts_behind_it() {
    let report = json!({
        "ok": true, "ready": true, "checks": [], "nextSteps": [], "warnings": [],
        "capabilities": {"nativeDialogs": {"state": "unknown", "reason": "Not checked"}},
        "session": {"user": {"name": "atlas", "uid": 502},
                    "display": {"width": 2560.0, "height": 1440.0},
                    "daemon": {"pid": 7, "version": "0.1.0"},
                    "inputService": {"pid": 9, "ready": true}},
    });
    let shown = show(View::Doctor { verbose: true }, report).out;
    assert!(shown.contains("atlas (uid 502)"));
    assert!(shown.contains("pid 7 · version 0.1.0"));
    assert!(shown.contains("unknown · Not checked"));
}

#[test]
fn roles_from_helper_apps_are_made_safe() {
    let shown = show(
        View::Action("Clicked".into()),
        json!({"ok": true, "element": {"role": "AX\u{1b}]0;x\u{7}", "title": "Save"}}),
    );
    assert!(!shown.out.contains('\u{1b}'));
}

#[test]
fn status_is_a_short_list_or_says_the_helper_is_silent() {
    let shown = show(
        View::Status,
        json!({"ok": true, "user": {"name": "atlas"}, "session": {"onConsole": false},
               "display": {"width": 2560.0, "height": 1440.0},
               "permissions": {"screenCapture": true, "accessibility": false},
               "inputService": {"ok": true, "ready": true}, "daemon": {"version": "0.1.0"}}),
    );
    assert!(shown.out.contains("atlas · in the background"));
    assert!(shown.out.contains("Screen Recording ✓  Accessibility ✗"));
    assert!(shown.out.contains("Input     ready"));

    let silent = show(
        View::Status,
        json!({"ok": false, "code": "transport", "error": "No answer"}),
    );
    assert!(silent.err.starts_with("✗ No answer"));
}

#[test]
fn config_names_the_settings_file() {
    let shown = show(
        View::Config,
        json!({"ok": true, "path": "/etc/atlas.toml", "config": {"helper_uid": 502}}),
    );
    assert!(shown.out.starts_with("Settings        /etc/atlas.toml"));
}

#[test]
fn elements_say_how_many_match_and_what_is_missing() {
    let all = show(
        View::Elements,
        json!({"ok": true, "pid": 9, "scanned": 2, "matching": 2, "more": 1,
               "scanIncomplete": true, "windows": [],
               "elements": [{"index": 0, "role": "AXButton", "name": "save",
                             "at": [11, 12], "size": [20, 20]}]}),
    );
    assert!(all.out.starts_with("Process 9 · 2 elements"));
    assert!(all.out.contains("save"));
    assert!(all.out.contains("Showing 1 of 2"));
    assert!(all.out.contains("The scan stopped early, so some"));

    let some = show(
        View::Elements,
        json!({"ok": true, "pid": 9, "scanned": 5, "matching": 0, "more": 0,
               "scanIncomplete": false, "elements": []}),
    );
    assert_eq!(some.out, "Process 9 · 0 of 5 elements match");
}

#[test]
fn the_elements_table_shows_each_click_point() {
    let compact = show(
        View::Elements,
        json!({"ok": true, "pid": 9, "scanned": 1, "matching": 1, "more": 0,
               "scanIncomplete": false, "windows": [],
               "elements": [{"index": 3, "role": "AXCheckBox", "name": "Bold",
                             "at": [400, 62], "size": [24, 22]}]}),
    );
    assert!(compact.out.contains("400, 62"), "{}", compact.out);

    let full = show(
        View::Elements,
        json!({"ok": true, "pid": 9, "scanned": 1, "matching": 1, "more": 0,
               "scanIncomplete": false,
               "elements": [{"index": 3, "depth": 2, "role": "AXCheckBox", "title": "Bold",
                             "frame": {"x": 388.0, "y": 51.0, "width": 24.0, "height": 22.0}}]}),
    );
    assert!(
        full.out.contains("400, 62"),
        "the middle of the frame: {}",
        full.out
    );
    assert!(!full.out.contains("388, 51"));
}

#[test]
fn a_scan_that_stopped_early_says_why() {
    let shown = show(
        View::Elements,
        json!({"ok": true, "pid": 9, "scanned": 0, "matching": 0, "more": 0,
               "scanIncomplete": true, "scan": {"reasons": ["count", "time"]},
               "elements": []}),
    );
    assert!(
        shown.out.contains(
            "The scan stopped early (the app has more elements than Atlas reads and it ran out \
             of time), so some elements may be missing."
        ),
        "{}",
        shown.out
    );
}

#[test]
fn a_refused_name_lists_what_it_may_have_meant() {
    let refused = Error::new(Code::Elements, "No visible element matches.").with(
        "candidates",
        json!([{"role": "AXMenuBarItem", "name": "Format", "at": [356, 11]},
               {"role": "AXButton", "name": "Form\u{1b}[31m", "at": [10, 20]}]),
    );
    console::set_colors_enabled_stderr(false);
    let shown = error(&refused);
    assert!(
        shown.contains("\n  · \"Format\" (menu bar item) at 356, 11"),
        "{shown}"
    );
    assert!(!shown.contains('\u{1b}'), "names are made safe: {shown}");

    let listed = show(
        View::Elements,
        json!({"ok": true, "pid": 9, "scanned": 4, "matching": 0, "more": 0,
               "scanIncomplete": false, "elements": [],
               "candidates": [{"role": "AXMenuBarItem", "name": "Format", "at": [356, 11]}]}),
    );
    assert!(
        listed
            .out
            .contains("· \"Format\" (menu bar item) at 356, 11"),
        "{}",
        listed.out
    );
}

#[test]
fn waits_name_the_window_or_its_closing() {
    let appeared = show(
        View::Wait { gone: false },
        json!({"ok": true, "waitedMs": 1200,
               "windows": [{"owner": "TextEdit", "title": "Untitled"}]}),
    );
    assert_eq!(appeared.out, "✓ TextEdit \"Untitled\" appeared after 1.2 s");

    let closed = show(
        View::Wait { gone: true },
        json!({"ok": true, "waitedMs": 5}),
    );
    assert_eq!(closed.out, "✓ The window closed after 5 ms");

    let late = show(
        View::Wait { gone: false },
        json!({"ok": false, "code": "wait-failed", "timedOut": true, "error": "No matching window appeared in time", "uncertain": false}),
    );
    assert!(
        late.err
            .starts_with("✗ No matching window appeared in time")
    );
}

#[test]
fn typing_that_stops_part_way_says_so() {
    let shown = show(
        View::Type,
        json!({"ok": false, "typed": 10, "batches": 2, "confirmedBatches": 1,
               "error": "No reply", "uncertain": true}),
    );
    assert!(
        shown
            .err
            .starts_with("✗ Typed 10 characters before it stopped: No reply")
    );
    assert!(shown.err.contains("don't repeat it"));

    let busy = show(
        View::Type,
        json!({"ok": false, "typed": 0, "batches": 1, "confirmedBatches": 0,
               "code": "busy", "error": "Someone has control", "uncertain": false}),
    );
    assert!(
        busy.err.contains("Release it there, or wait"),
        "{}",
        busy.err
    );
}

#[test]
fn agent_changes_list_each_agent() {
    let shown = show(
        View::AgentsChange,
        json!({"ok": false, "agents": [
            {"agent": "claude", "ok": true, "connected": true},
            {"agent": "codex", "ok": false, "error": "not installed"},
        ]}),
    );
    assert!(shown.out.starts_with("✓ Claude Code connected"));
    assert!(shown.out.contains("New sessions know how to use Atlas."));
    assert!(
        shown
            .out
            .ends_with("atlas agents project tells its agents to test with it.")
    );
    assert!(shown.err.starts_with("✗ Codex: not installed"));
}

#[test]
fn a_project_says_which_files_carry_atlas() {
    let shown = show(
        View::AgentsProject,
        json!({"ok": true, "files": [
            {"path": "/p/AGENTS.md", "change": "added"},
            {"path": "/p/CLAUDE.md", "change": "unchanged"},
        ]}),
    );
    assert_eq!(
        shown.out,
        "✓ Added Atlas to AGENTS.md\n✓ CLAUDE.md already has Atlas\n\
         Agents working here will now test changes in the helper."
    );
}

#[test]
fn switching_lists_accounts_or_says_what_to_choose() {
    let list = show(
        View::Switch,
        json!({"ok": true, "accounts": [{"name": "atlas", "realName": "Helper", "helper": true}]}),
    );
    assert!(list.out.contains("Helper  atlas    helper"));

    let none = show(View::Switch, json!({"ok": true, "accounts": []}));
    assert_eq!(none.out, "No other account to switch to.");

    let locked = show(
        View::Switch,
        json!({"ok": true, "locked": true, "choose": "Helper"}),
    );
    assert_eq!(
        locked.out,
        "✓ Screen locked. Click Switch User and choose Helper."
    );
}

#[test]
fn removal_and_login_read_as_sentences() {
    let removed = show(
        View::Uninstall,
        json!({"ok": true, "kept": {"helperAccount": "atlas",
                                     "remove": "sudo sysadminctl -deleteUser atlas"}}),
    );
    assert!(
        removed.out.contains("To delete the helper account too: ")
            && removed.out.contains("(it's logged out, so this is safe).")
    );

    let deleted = show(
        View::Uninstall,
        json!({"ok": true, "kept": {"helperAccount": null, "remove": null}}),
    );
    assert!(
        !deleted.out.contains("To delete"),
        "no way to delete what is gone"
    );

    let on = show(
        View::Login,
        json!({"ok": true, "available": true, "enabled": true}),
    );
    assert_eq!(on.out, "✓ The menu bar opens at login");
}

#[test]
fn the_menu_bar_says_only_that_it_already_runs_and_the_viewer_that_it_opened() {
    let running = show(
        View::Menubar,
        json!({"ok": true, "alreadyRunning": true, "running": true, "pid": 42}),
    );
    assert_eq!(
        running.out,
        "✓ The menu bar is already running. Click Atlas at the top of the screen; quit it from its menu."
    );
    let rendered = show(View::Menubar, json!({"ok": true, "paths": ["/tmp/a.png"]}));
    assert_eq!(rendered, Rendered::default());

    let opened = show(View::Monitor, json!({"ok": true, "viewer": "open"}));
    assert_eq!(
        opened.out,
        "✓ The viewer is open. Watch the helper there, or take control."
    );
}

#[test]
fn readiness_marks_each_item() {
    let shown = show(
        View::Readiness,
        json!({"ok": true, "items": [
            {"name": "git", "status": "ready", "detail": "git 2.54"},
            {"name": "nope", "status": "missing", "detail": ""},
        ], "notVerified": ["application launch"]}),
    );
    assert_eq!(
        shown.out,
        "✓ git  ready · git 2.54\n✗ nope  missing\nNot checked: application launch."
    );
}

#[test]
fn a_fix_is_left_out_only_when_the_message_says_what_to_run() {
    console::set_colors_enabled(false);

    // Naming the helper account (often "atlas") is not a fix.
    let named = Error::new(Code::Switch, "No account named atlas to switch to");
    assert!(error(&named).contains("→ Use the Apple menu"));

    let says = Error::new(Code::Init, "Run atlas init again.");
    assert_eq!(error(&says), "✗ Run atlas init again.");

    let stopped = Error::new(Code::Cancelled, "Stopped.");
    assert_eq!(error(&stopped), "– Stopped.");
}

#[test]
fn environment_steps_say_what_changes_and_what_comes_next() {
    let id = "a".repeat(48);
    let preview = show(
        View::Environment,
        json!({"ok": true, "applied": false, "planId": id,
               "files": [{"operation": "create", "target": "Notes/note.txt"}]}),
    );
    assert!(preview.out.contains("Would copy into the helper (1 file):"));
    assert!(preview.out.contains("add              ~/Notes/note.txt"));
    assert!(
        preview
            .out
            .ends_with(&format!("To copy: atlas environment apply {id}"))
    );

    let applied = show(
        View::Environment,
        json!({"ok": true, "applied": true, "phase": "applied", "resumeRequired": false,
               "planId": id, "files": [{"operation": "create", "target": "Notes/note.txt"}]}),
    );
    assert!(applied.out.starts_with("✓ Copied into the helper"));
    assert!(
        applied
            .out
            .ends_with(&format!("To undo: atlas environment rollback {id} --apply"))
    );

    let undone = show(
        View::Environment,
        json!({"ok": true, "rolledBack": true, "planId": id,
               "completedTargets": ["Notes/note.txt"], "files": []}),
    );
    assert_eq!(undone.out, "✓ Undone in the helper\n  ~/Notes/note.txt");

    let conflict = show(
        View::Environment,
        json!({"ok": false, "applied": false, "planId": id,
               "files": [{"operation": "conflict", "target": "Notes/note.txt"}]}),
    );
    assert!(conflict.err.starts_with(
        "✗ Nothing was copied: 1 file in the helper already exists or changed there since Atlas copied it."
    ));

    let stopped = show(
        View::Environment,
        json!({"ok": false, "error": "Disk full", "phase": "applying", "resumeRequired": true,
               "planId": id, "files": [{"operation": "create", "target": "a"}]}),
    );
    assert!(
        stopped
            .err
            .starts_with("✗ Copying stopped part way: Disk full")
    );
    assert!(
        stopped
            .err
            .ends_with(&format!("To finish: atlas environment apply {id}"))
    );
}

#[test]
fn an_undo_names_the_files_it_keeps_and_a_stuck_copy_says_to_undo_it() {
    let id = "a".repeat(48);
    let undone = show(
        View::Environment,
        json!({"ok": true, "rolledBack": true, "planId": id, "files": [],
               "completedTargets": ["b"], "keptTargets": ["a"]}),
    );
    assert_eq!(
        undone.out,
        "✓ Undone in the helper\n  ~/b\nKept 1 file the helper changed since the copy:\n  ~/a"
    );

    let preview = show(
        View::Environment,
        json!({"ok": true, "preview": true, "planId": id, "files": [],
               "pendingTargets": [], "keptTargets": ["a"]}),
    );
    assert!(
        preview
            .out
            .starts_with("✓ No files change back in the helper\nIt keeps 1 file")
    );
    assert!(
        preview
            .out
            .ends_with(&format!("To undo: atlas environment rollback {id} --apply"))
    );

    let stuck = show(
        View::Environment,
        json!({"ok": false, "error": "a changed", "phase": "applying", "resumeRequired": true,
               "rollbackRequired": true, "planId": id, "files": []}),
    );
    assert!(
        stuck
            .err
            .starts_with("✗ Copying stopped part way: a changed")
    );
    assert!(stuck.err.ends_with(&format!(
        "To finish: atlas environment rollback {id} --apply"
    )));
}

#[test]
fn closing_everything_names_what_it_quit() {
    let some = show(
        View::Close,
        json!({"ok": true, "closed": true, "apps": ["Calendar", "Notes"], "removed": []}),
    );
    assert_eq!(some.out, "✓ Closed Calendar, Notes");

    let none = show(
        View::Close,
        json!({"ok": true, "closed": false, "apps": [], "removed": []}),
    );
    assert_eq!(none.out, "✓ No apps were open");

    let partly = show(
        View::Close,
        json!({"ok": true, "closed": true, "apps": ["Notes"], "skipped": ["Safari"],
               "removed": ["/Users/Shared/Atlas/handoff/x"]}),
    );
    assert_eq!(
        partly.out,
        "✓ Closed Notes · removed Atlas's copies\n! Left Safari: it changed before it could be quit"
    );

    let one = show(
        View::Close,
        json!({"ok": true, "closed": true, "app": "TextEdit", "removed": []}),
    );
    assert_eq!(one.out, "✓ Closed TextEdit");
}

#[test]
fn listed_copies_into_the_helper_read_as_a_table_of_ids_states_and_targets() {
    let id = "3c1f00000000000000000000000000000000000000000000";
    let listed = show(
        View::Plans,
        json!({"ok": true, "more": 2, "plans": [{"planId": id, "state": "applying",
            "updatedAt": 1, "files": [{"target": ".config/probe/settings.toml", "operation": "create"}],
            "session": null, "thisTask": false}]}),
    );
    assert!(listed.out.contains(id), "{}", listed.out);
    assert!(
        listed.out.contains("stopped while copying"),
        "{}",
        listed.out
    );
    assert!(
        listed.out.contains(".config/probe/settings.toml"),
        "{}",
        listed.out
    );
    assert!(listed.out.ends_with("2 older not shown"), "{}", listed.out);

    let none = show(
        View::Plans,
        json!({"ok": true, "plans": [], "more": 0, "hidden": 0, "unreadable": 0}),
    );
    assert_eq!(none.out, "✓ Nothing has been copied into the helper");
}

#[test]
fn a_listing_that_could_not_read_everything_never_reads_as_nothing_copied() {
    let unread = show(
        View::Plans,
        json!({"ok": true, "plans": [], "more": 0, "hidden": 0, "unreadable": 1}),
    );
    assert_eq!(
        unread.out,
        "No copy into the helper is listed\n! 1 record could not be read, so this list may be missing some"
    );

    let hidden = show(
        View::Plans,
        json!({"ok": true, "plans": [], "more": 0, "hidden": 3, "unreadable": 0}),
    );
    assert_eq!(
        hidden.out,
        "No copy into the helper is listed\n3 undone completely not shown (--all lists them)"
    );

    let listed = show(
        View::Plans,
        json!({"ok": true, "more": 0, "hidden": 0, "unreadable": 2, "plans": [{"planId": "3c1f",
            "state": "applied", "files": [{"target": "a", "operation": "create"}]}]}),
    );
    assert!(
        listed
            .out
            .ends_with("! 2 records could not be read, so this list may be missing some"),
        "{}",
        listed.out
    );
}

#[test]
fn activity_names_each_task_and_what_it_left() {
    let one = show(
        View::Activity,
        json!({"ok": true,
            "session": {"key": "3f9a1c07b2e4", "name": "Claude Code · left", "this": true},
            "counts": {"open": 2}, "recent": [], "screenshots": [], "copies": [], "plans": [],
            "uncertain": 0,
            "tasks": [
                {"key": "5be0d1a2c3f4", "name": "settings-sheet",
                    "leftovers": {"copies": 1, "plans": 2}},
                {"key": "6cf1e2b3d4a5", "name": "review", "leftovers": {"copies": 0, "plans": 0}}]}),
    );
    assert_eq!(
        one.out,
        "Claude Code · left — this session\n  2 opens\n  \
         Task settings-sheet: left 1 open copy and 2 environment plans\n  \
         Task review: nothing left"
    );
    assert!(!one.out.contains("5be0d1a2c3f4"), "no keys in words");
}

#[test]
fn a_close_says_which_copies_it_kept_and_which_it_could_not_remove() {
    let shown = show(
        View::Close,
        json!({"ok": true, "closed": true, "app": "TextEdit", "removed": [], "keptOthers": 2,
               "failed": [{"path": "/Users/Shared/Atlas/Open/x/notes.rtf", "error": "Permission denied"}]}),
    );
    assert_eq!(
        shown.out,
        "✓ Closed TextEdit · kept 2 copies another session made\n! Could not remove 1 copy; run the same close again"
    );
}

/// An open says what it opened and where the copy went; a failed one still
/// says where the copy went, since close is what removes it.
#[test]
fn an_open_names_its_copy_whether_or_not_it_launched() {
    let copied = json!(["/Users/Shared/Atlas/Open/abc/App.app"]);
    let opened = show(
        View::Open("App.app".into()),
        json!({"ok": true, "opened": true, "copied": copied}),
    );
    assert!(opened.out.starts_with("✓ Opened App.app"), "{}", opened.out);
    assert!(opened.out.contains("copied first to"), "{}", opened.out);

    let refused = show(
        View::Open("App.app".into()),
        json!({"ok": false, "opened": false, "code": "spawn-failed",
           "error": "Unable to find application named 'App'", "copied": copied,
           "uncertain": false}),
    );
    assert!(
        refused
            .err
            .starts_with("✗ Unable to find application named 'App'"),
        "{}",
        refused.err
    );
    assert!(refused.err.contains("copied first to"), "{}", refused.err);

    let plain = show(
        View::Open("Calculator".into()),
        json!({"ok": false, "opened": false, "code": "spawn-failed",
           "error": "Unable to find application named 'Calculator'", "uncertain": false}),
    );
    assert!(!plain.err.contains("copied"), "{}", plain.err);
}

#[test]
fn the_last_login_reads_one_step_a_line() {
    let told = show(
        View::LoginTiming,
        json!({"ok": true, "found": true,
               "steps": {"passwordToUpdater": 0.5, "updater": 0.4, "setupAssistant": false,
                         "loginCompleteToDesktop": 1.0, "desktopToAtlas": 2.5,
                         "passwordToDesktop": 4.0},
               "slowestPlugin": {"plugin": "com.apple.dock.DockUpdater", "phase": "User",
                                 "seconds": 0.3},
               "unfinished": []}),
    );
    assert!(
        told.out.contains("Password → desktop          4.0 s"),
        "{}",
        told.out
    );
    assert!(told.out.contains("Setup Assistant             skipped"));
    assert!(told.out.contains("slowest: com.apple.dock.DockUpdater"));

    let hung = show(
        View::LoginTiming,
        json!({"ok": true, "found": true, "steps": {},
               "unfinished": ["com.apple.dock.SpacesUpdater"]}),
    );
    assert!(hung.out.contains("never finished"), "{}", hung.out);
    assert!(
        hung.out
            .contains("Never finished: com.apple.dock.SpacesUpdater")
    );

    let none = show(View::LoginTiming, json!({"ok": true, "found": false}));
    assert!(none.out.contains("No login in the last 2 hours"));
}

#[test]
fn a_refusal_names_the_session_it_is_about_in_words() {
    console::set_colors_enabled_stderr(false);
    let held = Error::new(Code::Busy, "Another Atlas command kept control.")
        .held_by(crate::error::Holder::Agent)
        .with(
            "heldBy",
            json!({"session": "c71d0e5a9b28", "name": "Codex · right"}),
        );
    let said = error(&held);
    assert!(said.contains("Held by Codex · right."), "{said}");
    assert!(!said.contains("c71d0e5a9b28"));

    let left = Error::new(Code::Uncertain, "Unknown")
        .with("causedBy", json!(["Claude Code · left", "Pi"]));
    assert!(error(&left).contains("Left by Claude Code · left and Pi."));

    let shown = show(
        View::Type,
        json!({"ok": false, "code": "focus-changed", "error": "Calculator has keyboard focus",
            "lastUsedBy": {"session": "c71d0e5a9b28", "name": "Codex · right"}}),
    );
    assert!(
        shown.err.contains("Last used by Codex · right."),
        "{}",
        shown.err
    );
    assert!(!error(&Error::new(Code::Busy, "Wait")).contains("by"));
}

/// A person reading a focus refusal is told what JSON tells an agent: what
/// was in the way, and to send only the rest when part was sent.
#[test]
fn a_focus_refusal_card_advises_by_what_was_in_the_way_and_what_was_sent() {
    let left = show(
        View::Type,
        json!({"ok": false, "code": "focus-changed", "reason": "field", "typed": 16,
            "partial": true, "error": "The field with keyboard focus is not the one named First"}),
    );
    let advice = crate::ui::hints::focus_changed(true, crate::ui::hints::InTheWay::Field);
    assert!(left.err.contains(&advice), "{}", left.err);
    assert!(left.err.contains("send only the rest"), "{}", left.err);

    let sheet = error(
        &Error::new(Code::FocusChanged, "A sheet is open over window 4120").with("reason", "sheet"),
    );
    assert!(sheet.contains("deal with the sheet"), "{sheet}");
    assert!(!sheet.contains("Bring the app to the front"), "{sheet}");
}

#[test]
fn activity_reads_as_words_for_one_session_or_each() {
    let one = show(
        View::Activity,
        json!({"ok": true,
            "session": {"key": "3f9a1c07b2e4", "name": "Claude Code · left", "this": true},
            "counts": {"click": 4, "open": 1, "screenshot": 6, "type": 2},
            "recent": [{"says": "took a screenshot", "ok": true, "ageSeconds": 40}],
            "screenshots": [], "copies": [{"app": "TextEdit"}],
            "plans": [{"planId": "3c1f00000000000000000000000000000000000000000000", "phase": "applied"}],
            "uncertain": 0}),
    );
    assert_eq!(
        one.out,
        "Claude Code · left — this session\n  4 clicks · 1 open · 6 screenshots · 2 typed\n  \
         Open copies: TextEdit\n  Environment plans: 3c1f… applied\n  Last: took a screenshot, 40 s ago"
    );
    assert!(!one.out.contains("3f9a1c07b2e4"), "no keys in words");

    let everyone = show(
        View::Activity,
        json!({"ok": true, "session": null, "recent": [], "sessions": [
            {"key": "c71d0e5a9b28", "name": "Codex · right", "holdsHelper": true,
                "leftovers": {"copies": 0, "plans": 0, "uncertain": 0}},
            {"key": "3f9a1c07b2e4", "name": "Claude Code · left",
                "leftovers": {"copies": 1, "plans": 1, "uncertain": 0}},
        ]}),
    );
    assert_eq!(
        everyone.out,
        "Codex · right, has control now\nClaude Code · left — left 1 open copy and 1 environment plan"
    );
    assert!(
        show(View::Activity, json!({"ok": true, "sessions": []}))
            .out
            .contains("No agent session")
    );
}

#[test]
fn activity_gives_the_restart_step_only_when_the_reply_does() {
    let unknown = |this: bool, next: bool| {
        let mut value = json!({"ok": true, "session": {"name": "Claude Code · left", "this": this},
            "counts": {}, "recent": [], "copies": [], "plans": [], "uncertain": 1});
        if next {
            value["next"] = json!(crate::ui::hints::UNKNOWN_OWN);
        }
        show(View::Activity, value).out
    };

    let own = unknown(true, true);
    assert!(
        own.contains("1 unknown action of this session: take a screenshot, then run atlas restart"),
        "{own}"
    );
    let shared = unknown(true, false);
    assert!(
        shared.contains("1 unknown action of this session") && !shared.contains("restart"),
        "{shared}"
    );
    let other = unknown(false, false);
    assert!(
        other.contains("1 unknown action")
            && !other.contains("this session")
            && !other.contains("restart"),
        "{other}"
    );
}

#[test]
fn a_cleanup_reads_kept_apart_from_failed_and_names_what_it_leaves() {
    let preview = show(
        View::Cleanup,
        json!({"ok": true, "preview": true, "steps": [
            {"n": 1, "do": "quit", "apps": [{"app": "Probe", "pid": 812}]},
            {"n": 2, "do": "undo-plan", "planId": "3c1f", "targets": [".config/probe/settings.toml"]}],
            "left": [{"app": "TextEdit", "why": "opened by name"}]}),
    );
    assert!(
        preview.out.contains("atlas cleanup --apply"),
        "{}",
        preview.out
    );
    assert!(
        preview.out.contains("1. Quit Probe (812)"),
        "{}",
        preview.out
    );
    assert!(
        preview.out.contains(".config/probe/settings.toml"),
        "{}",
        preview.out
    );
    assert!(
        preview.out.contains("not quit: TextEdit"),
        "{}",
        preview.out
    );
    assert!(preview.err.is_empty());

    let run = show(
        View::Cleanup,
        json!({"ok": false, "code": "cleanup", "uncertain": false, "state": "incomplete",
            "error": "Cleanup is not finished: 1 step failed.", "steps": [
            {"n": 1, "do": "remove-copy", "path": "/Users/Shared/Atlas/Open/9c1e", "state": "kept",
             "reason": "another-session"},
            {"n": 2, "do": "remove-copy", "path": "/Users/Shared/Atlas/Open/4d2a", "state": "failed",
             "reason": "in-use", "retry": true}], "left": []}),
    );
    assert!(
        run.out.contains("kept: another session made it"),
        "{}",
        run.out
    );
    assert!(
        run.out.contains("couldn't: still in use; try again"),
        "{}",
        run.out
    );
    assert!(run.err.contains("1 step failed"), "{}", run.err);

    let nothing = show(
        View::Cleanup,
        json!({"ok": true, "preview": false, "steps": [], "left": []}),
    );
    assert_eq!(nothing.out, "✓ Nothing to clean up in the helper");
}

#[test]
fn a_cleanup_says_where_an_install_is_held_and_when_one_was_already_gone() {
    let held = "/Users/atlas/.config/atlas/installs/held/9c1e/Probe.app";
    let run = show(
        View::Cleanup,
        json!({"ok": true, "preview": false, "state": "complete", "steps": [
            {"n": 1, "do": "remove-install", "app": "Probe", "state": "done", "held": held},
            {"n": 2, "do": "remove-install", "app": "Opie", "state": "done", "alreadyGone": true},
            {"n": 3, "do": "remove-install", "app": "Notes", "state": "kept",
             "reason": "other-volume"}], "left": []}),
    );
    assert!(
        run.out.contains(&format!(
            "1. Move Probe from Applications aside in the helper · held at {held}; removed after 7 days"
        )),
        "{}",
        run.out
    );
    assert!(
        run.out
            .contains("2. Move Opie from Applications aside in the helper · already gone"),
        "{}",
        run.out
    );
    assert!(
        run.out
            .contains("kept: it could not be moved aside without copying"),
        "{}",
        run.out
    );
}

#[test]
fn an_install_says_where_it_went_and_only_when_macos_has_not_registered_it() {
    let installed = |registered: bool| {
        show(
            View::InstallApp,
            json!({"ok": true, "installed": true, "app": "Probe",
                "path": "/Users/atlas/Applications/Probe.app", "registered": registered}),
        )
        .out
    };

    let registered = installed(true);
    assert!(
        registered.contains("Installed Probe at /Users/atlas/Applications/Probe.app"),
        "{registered}"
    );
    assert!(
        registered.contains("atlas cleanup --apply moves it aside"),
        "{registered}"
    );
    assert!(!registered.contains("not registered"), "{registered}");
    let unregistered = installed(false);
    assert!(
        unregistered.contains("macOS has not registered it yet"),
        "{unregistered}"
    );
}

#[test]
fn a_stopped_job_says_what_it_left_running() {
    let shown = show(
        View::Job,
        json!({"ok": true, "id": "7c2e91a04b3f", "state": "ended", "how": "stopped",
            "left": [{"pid": 4377, "name": "sleep"}]}),
    );
    assert!(
        shown.out.contains("Job 7c2e91a04b3f ended: stopped"),
        "{}",
        shown.out
    );
    assert!(
        shown.out.contains("never signalled: sleep (4377)"),
        "{}",
        shown.out
    );

    let shown = show(
        View::Jobs,
        json!({"ok": true, "jobs": [
            {"id": "7c2e91a04b3f", "name": "web", "state": "ended", "how": "leader-gone",
                "left": [{"pid": 9, "name": "node"}]},
            {"id": "000000000000", "name": "db", "state": "stopping", "refused": "identity",
                "mismatch": true},
        ]}),
    );
    assert!(
        shown.out.contains("Atlas signalled nothing"),
        "{}",
        shown.out
    );
    assert!(shown.out.contains("node (9)"));
    assert!(shown.out.contains("not signalled: identity"));
    assert!(shown.out.contains("another process than the task's"));
}

#[test]
fn a_collection_lists_its_copies_and_what_it_did_not_keep() {
    let shown = show(
        View::Collect,
        json!({"ok": false, "code": "collect", "error": "1 of 2 files was not collected",
            "uncertain": false,
            "collected": [{"path": "/x/files/blob.bin", "size": 3, "sha256": "ab"}],
            "failed": [{"source": "grow.log", "reason": "changing",
                "error": "grow.log changed in the last 2 seconds"}]}),
    );
    assert!(
        shown
            .out
            .contains("/x/files/blob.bin (3 bytes, SHA-256 ab)")
    );
    assert!(shown.out.contains("grow.log changed"));
    assert!(shown.err.contains("1 of 2 files was not collected"));
}

#[test]
fn a_blocked_cleanup_step_says_why_and_what_it_kept() {
    let shown = show(
        View::Cleanup,
        json!({"ok": false, "code": "cleanup", "preview": false, "state": "blocked",
        "error": "Cleanup is not finished", "uncertain": false, "left": [],
        "steps": [
            {"n": 1, "do": "check-outputs", "state": "failed", "reason": "output-not-kept",
                "notKept": [{"source": "/Users/atlas/a.pdf"}]},
            {"n": 2, "do": "stop-job", "id": "7c2e91a04b3f", "name": "web", "state": "done",
                "left": [{"pid": 77, "name": "sleep"}]},
            {"n": 3, "do": "undo-plan", "planId": "p", "state": "blocked",
                "reason": "output-not-kept"},
            {"n": 4, "do": "remove-copy", "path": "/c", "state": "blocked", "reason": "daemon"},
        ]}),
    );
    assert!(
        shown.out.contains("kept in the helper: /Users/atlas/a.pdf"),
        "{}",
        shown.out
    );
    assert!(shown.out.contains("never signalled: sleep (77)"));
    assert!(
        shown
            .out
            .contains("not run: a collected copy is missing or changed")
    );
    assert!(
        shown
            .out
            .contains("not run: the helper could not be reached")
    );
    assert!(!shown.out.contains("agents are stopped"));
}
