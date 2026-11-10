use super::*;

#[test]
fn the_guide_names_only_real_commands() {
    for command in [
        "doctor",
        "readiness",
        "screenshot",
        "windows",
        "elements",
        "click",
        "type",
        "key",
        "drag",
        "scroll",
        "open",
        "wait",
        "run",
        "restart",
        "close",
        "cleanup",
        "activity",
        "install-app",
        "environment list",
    ] {
        assert!(GUIDE.contains(&format!("atlas {command}")), "{command}");
    }

    for command in [
        "doctor",
        "windows",
        "screenshot",
        "elements",
        "click",
        "wait",
        "open",
        "run",
        "restart",
        "close",
        "cleanup",
        "activity",
        "monitor",
    ] {
        assert!(CORE.contains(&format!("atlas {command}")), "{command}");
    }
}

/// The core with its lines joined, so a phrase may wrap anywhere.
fn core_text() -> String {
    CORE.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What the skill says of each of its [`REFERENCES`].
const ABOUT: [&str; 5] = [
    "Test your own app.",
    "Give the app what it needs: copy files in and roll them back.",
    "Limits, and details of typing, keys, scrolling and `atlas run`.",
    "every command and its options.",
    "the error reference, every code and what to do.",
];

/// When a skill loads: what Atlas is for, in the words an agent matches.
const SKILL_DESCRIPTION: &str = "See and use macOS GUIs in a hidden second login (the helper) \
without touching the person's desktop, windows, keyboard or mouse. Use it whenever you change \
something a person could see in a Mac app (a window, dialog, menu, menu bar item, layout or \
icon), open the build in the helper and check it before saying it works. Also use it to \
reproduce on-screen bugs, run installers or anything that opens windows or takes focus, test \
a web app in Google Chrome or Firefox, take screenshots, and click or type in apps. Also use it when the person \
asks to watch the helper, see what you are doing or take over: open the live view for them. Not for unit tests, builds \
or reading files.";

/// Where the skill's references live, beside its SKILL.md.
fn references_folder() -> std::path::PathBuf {
    atlas_core::app_bundle::checkout().join("agents/atlas/skills/atlas/references")
}

#[test]
fn the_skill_ends_with_every_reference_and_what_it_is_for() {
    let skill = skill();
    let (_, rest) = skill.split_once("\n\n## References\n").unwrap();
    let mut expected = String::from("\nRead one when the task needs it:\n\n");
    for (file, about) in REFERENCES.iter().zip(ABOUT) {
        expected.push_str(&format!(
            "- [references/{file}](references/{file}): {about}\n"
        ));
    }
    assert_eq!(rest, expected);
}

/// Every agent loads the whole skill at the start of each task. When it
/// grows past this, move detail into a reference rather than raise it.
#[test]
fn the_skill_stays_near_two_thousand_tokens() {
    let size = skill().len();
    assert!(size <= 8 * 1024, "the skill is {size} bytes");
}

#[test]
fn the_skill_core_keeps_every_rule_an_agent_needs_from_the_start() {
    let core = core_text();
    let actor = ["ATLAS", "ACTOR"].join("_");
    for rule in [
        "busy",
        "holder",
        "stopped",
        "\"uncertain\": true",
        "Do not run `atlas restart`",
        "on-console",
        "blocked",
        "session-changed",
        actor.as_str(),
        "privacy settings",
        "credentials",
        "atlas switch",
        "atlas logout",
        "Never repeat",
        "each app you opened",
        "unsaved work",
        "without a screenshot that shows it",
        "show_screen",
        "&&",
        "unchanged: true",
        "focus-changed",
        "Do what `next` says, then send only what was not.",
        "--app A",
        "`atlas click --app A --window N X Y`",
        "--wait",
        "`waited` says how long Atlas waited",
        "if `heldBy` is you, wait for your own command",
        "your shell tool's limit",
        "never end your turn with an Atlas command still running",
        "If your host stopped a command of yours, run `atlas doctor`; restart only when it says all is yours.",
        "yours: true",
        "Touch only windows you opened",
        "`unread: withheld`",
        "ask the person to unlock",
        "`ATLAS_SESSION=NAME` before every `atlas` command",
        "put `ATLAS_TASK=NAME` on every command too, cleanup included",
        "atlas activity",
        "atlas job start",
        "atlas collect",
        "check it, then `atlas cleanup --apply` stops its jobs",
    ] {
        assert!(core.contains(rule), "{rule}");
    }
}

#[test]
fn the_guides_limit_window_memory_to_untargeted_points_and_name_windows_in_shared_apps() {
    for guide in [CORE.as_str(), GUIDE.as_str()] {
        assert!(guide.contains("Untargeted points"));
        assert!(guide.contains("bypasses this memory"));
        assert!(
            guide
                .lines()
                .any(|line| { line.contains("shared app") && line.contains("`--window") })
        );
    }
}

/// Spans in backticks in the full guide and the error reference.
fn shown_spans() -> impl Iterator<Item = &'static str> {
    [GUIDE.as_str(), ERRORS]
        .into_iter()
        .flat_map(|text| text.split('`').skip(1).step_by(2))
}

/// Between two of an agent's commands another may bring its own window
/// forward, so every keyboard or pointer example names the app it is for,
/// and every save or close names the window too, unless it is Finder's
/// or a sheet's.
#[test]
fn every_keyboard_and_pointer_example_the_guide_shows_names_its_app() {
    let examples: Vec<&str> = shown_spans()
        .filter(|span| {
            let mut words = span.split_whitespace();
            let command = matches!(
                (words.next(), words.next()),
                (
                    Some("atlas"),
                    Some("type" | "key" | "scroll" | "drag" | "click")
                )
            );
            // A command named in prose, with at most its flags, shows
            // no input.
            command && words.any(|word| !word.starts_with('-'))
        })
        .collect();
    assert!(examples.len() >= 10, "{examples:?}");
    for span in &examples {
        assert!(span.contains(" --app "), "`{span}` names no app");
        if span.contains("cmd+s") || span.contains("cmd+w") {
            assert!(span.contains(" --window N "), "`{span}` names no window");
        }
    }

    // A key named on its own (`key escape`) is an example too.
    for span in shown_spans() {
        assert!(!span.starts_with("key "), "`{span}` names no app");
    }
}

/// Every code the core gives its own bullet is one Atlas reports.
#[test]
fn every_code_the_core_bullets_is_a_real_code() {
    let (_, section) = CORE.split_once("## When something goes wrong").unwrap();
    let (section, _) = section.split_once("\n## ").unwrap();
    let mut codes = Vec::new();
    for bullet in section.lines().filter(|line| line.starts_with("- ")) {
        // Up to the first colon outside backticks.
        let mut quoted = false;
        let end = bullet
            .char_indices()
            .find(|&(_, c)| {
                quoted ^= c == '`';
                c == ':' && !quoted
            })
            .map_or(bullet.len(), |(index, _)| index);
        let head = &bullet[..end];
        codes.extend(
            head.split('`')
                .skip(1)
                .step_by(2)
                .filter(|span| span.chars().all(|c| c.is_ascii_lowercase() || c == '-')),
        );
    }
    for code in ["uncertain", "busy", "focus-changed", "stopped", "cancelled"] {
        assert!(codes.contains(&code), "{code}: {codes:?}");
    }
    for code in codes {
        assert!(atlas_core::error::CODES.contains(&code), "{code}");
    }
}

/// What an agent sharing the helper reads only when it needs it: how
/// long Atlas waits, each host's time limit, and what not to touch.
#[test]
fn the_limits_say_how_long_to_wait_and_how_to_work_beside_another_agent() {
    let guide = GUIDE.split_whitespace().collect::<Vec<_>>().join(" ");
    for section in ["Time limits", "Working beside another agent"] {
        assert!(GUIDE.contains(&format!("\n### {section}\n")), "{section}");
    }
    for said in [
        "`ATLAS_WAIT`",
        "128 plus the signal's number",
        "a command that finished in time prints its ordinary result",
        "a longer `--wait` does not change that",
        "about 10 seconds to stop what is left",
        "Never end your turn with an Atlas command still running.",
        "`--app` checks the app, not the window.",
        "`click --name` takes no `--window`",
        "Your window is the one `atlas windows` did not list before you opened it.",
        "`causedBy`",
        "An `uncertain` you did not cause is not yours to recover",
        "Never restart over `yours: false`",
    ] {
        assert!(guide.contains(said), "{said}");
    }
}

/// A cleanup with no task named covers every task of the session, so
/// the guide never calls it this task's.
#[test]
fn a_cleanup_with_no_task_is_said_to_cover_the_whole_session() {
    let guide = GUIDE.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(core_text().contains(
        "`atlas cleanup` shows what else this session left (with `ATLAS_TASK`, that task's alone)"
    ));
    assert!(guide.contains("A plain `atlas cleanup` covers the whole session"));
    assert!(guide.contains("keep the same `ATLAS_TASK` through cleanup"));
    for wrong in [
        "cleanup` shows what else this task",
        "cleanup` shows everything this task",
    ] {
        assert!(!guide.contains(wrong), "{wrong}");
    }
}

#[test]
fn no_step_quits_or_discards_what_the_task_did_not_open() {
    let core = core_text();
    for (index, _) in core.match_indices("close --all") {
        assert!(
            core[..index].ends_with("never run `atlas "),
            "close --all is taught: {}",
            &core[index.saturating_sub(60)..index]
        );
    }
    assert!(!core.to_lowercase().contains("cmd+backspace"));
}

#[test]
fn the_full_guide_is_the_core_then_every_reference_written_for_it() {
    let mut expected = format!("{}\n", CORE.as_str());
    for file in ["testing.md", "environment.md", "limits.md"] {
        assert!(REFERENCES.contains(&file), "{file}");
        let text = std::fs::read_to_string(references_folder().join(file)).unwrap();
        assert!(text.starts_with("## "), "{file} starts with its heading");
        expected.push_str(&text);
        expected.push('\n');
    }
    let rest = GUIDE.strip_prefix(&expected).unwrap();
    assert!(rest.starts_with("## References\n"), "{rest}");
    assert!(rest.contains("`atlas agent-guide --commands`"), "{rest}");
    assert!(rest.contains("`atlas agent-guide --errors`"), "{rest}");
}

#[test]
fn every_reference_the_skill_lists_is_in_its_folder_and_nothing_else_is() {
    let mut present: Vec<String> = std::fs::read_dir(references_folder())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    present.sort();
    let mut listed: Vec<String> = REFERENCES.iter().map(|file| (*file).to_owned()).collect();
    listed.sort();
    assert_eq!(present, listed);
}

/// The core sends an agent to these sections by name: each heads a part
/// of the full guide and is how the skill's References list names it.
#[test]
fn every_section_the_core_points_to_exists() {
    let core = core_text();
    for section in ["Test your own app", "Give the app what it needs", "Limits"] {
        assert!(core.contains(&format!("read {section}")), "{section}");
        assert!(GUIDE.contains(&format!("\n## {section}\n")), "{section}");
        assert!(
            ABOUT.iter().any(|about| about.starts_with(section)),
            "{section}"
        );
    }
}

#[test]
fn agents_are_told_to_read_both_output_streams_on_failure() {
    for text in [GUIDE.to_owned(), skill()] {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(text.contains("exit non-zero on failure; then read both stdout and stderr."));
    }
}

/// Agents read the skill's description from YAML front matter: it stays
/// one double-quoted line with nothing that would end or break it.
#[test]
fn the_skill_description_is_valid_front_matter() {
    assert!(!SKILL_DESCRIPTION.contains(['"', '\\', '\n']));
    assert!(skill().starts_with(&format!(
        "---\nname: atlas\ndescription: \"{SKILL_DESCRIPTION}\"\n---\n"
    )));
}

#[test]
fn the_guide_restarts_after_a_stopped_command_only_when_recovery_says_so() {
    assert!(core_text().contains(
        "If your host stopped a command of yours, run `atlas doctor`; restart only when it says all is yours."
    ));
}
