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

    let plain = Error::new("on-console", "The helper is on screen right now");
    assert_eq!(
        error(&plain),
        "✗ The helper is on screen right now\n  → Switch back to your own account. Atlas works while the helper is in the background."
    );

    let says = Error::new("config", "No configuration; run atlas init");
    assert_eq!(error(&says), "✗ No configuration; run atlas init");

    let unsure = Error::new("transport", "The helper stopped answering").uncertain();
    assert!(error(&unsure).contains("don't repeat it"));
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
fn the_elements_table_shows_each_centre_point() {
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
        "the frame's centre: {}",
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
    let refused = Error::new("elements", "No visible element matches.").with(
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
    assert!(shown.err.starts_with("✗ Typed 1 of 2 parts: No reply"));
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
    let named = Error::new("switch", "No account named atlas to switch to");
    assert!(error(&named).contains("→ Use the Apple menu"));

    let says = Error::new("init", "Run atlas init again.");
    assert_eq!(error(&says), "✗ Run atlas init again.");

    let stopped = Error::new("cancelled", "Stopped.");
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
