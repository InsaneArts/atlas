//! `atlas --help`: the commands in a few groups a person can scan,
//! built from each command's own one-line description so they never drift.
use super::cli::Cli;
use atlas::ui::{self, Format, style};
use clap::{
    CommandFactory,
    error::{ContextKind, ContextValue, ErrorKind},
};
use std::{io::IsTerminal, process::ExitCode};

/// Each visible command in exactly one group, in reading order.
pub const GROUPS: &[(&str, &[&str])] = &[
    ("Get started", &["init", "doctor", "agents", "menubar"]),
    (
        "Look at the helper",
        &[
            "screenshot",
            "windows",
            "elements",
            "apps",
            "status",
            "activity",
        ],
    ),
    (
        "Act in the helper",
        &[
            "click", "type", "key", "move", "drag", "scroll", "input", "open", "close", "cleanup",
            "wait", "run", "job", "collect",
        ],
    ),
    (
        "Prepare the helper",
        &["readiness", "environment", "install-app", "verify-native"],
    ),
    (
        "Take over or recover",
        &["switch", "monitor", "restart", "logout", "start-input"],
    ),
    (
        "Look after Atlas",
        &[
            "update",
            "uninstall",
            "install",
            "setup",
            "config",
            "agent-guide",
        ],
    ),
];

/// A few ways to use each command that takes arguments, shown under its
/// `--help` and in its usage errors.
const EXAMPLES: &[(&str, &[&str])] = &[
    (
        "doctor",
        &[
            "atlas doctor",
            "atlas doctor --verbose",
            "atlas doctor --login-timing",
        ],
    ),
    ("windows", &["atlas windows --app TextEdit"]),
    ("activity", &["atlas activity", "atlas activity --all"]),
    (
        "elements",
        &["atlas elements --app TextEdit --role AXButton"],
    ),
    (
        "click",
        &[
            "atlas click --app TextEdit --name Save",
            "atlas click 1200 880",
        ],
    ),
    ("type", &["atlas type \"hello\""]),
    ("key", &["atlas key cmd+s", "atlas key escape return"]),
    ("move", &["atlas move 640 400"]),
    ("drag", &["atlas drag 100 200 400 200"]),
    ("scroll", &["atlas scroll 640 400 --dy 3"]),
    ("input", &["atlas input @actions.json"]),
    (
        "open",
        &[
            "atlas open --app TextEdit",
            "atlas open ./build/Debug/MyApp.app --env DEBUG=1",
            "atlas open --app \"Google Chrome\" https://example.com",
        ],
    ),
    ("close", &["atlas close --app TextEdit"]),
    (
        "install-app",
        &[
            "atlas install-app ./build/Release/Probe.app",
            "atlas open --app Probe",
        ],
    ),
    (
        "cleanup",
        &[
            "atlas cleanup",
            "atlas cleanup --apply",
            "atlas cleanup --session 3f9a1c07b2e4 --apply",
        ],
    ),
    (
        "wait",
        &[
            "atlas wait --app TextEdit",
            "atlas wait --title Save --gone",
        ],
    ),
    (
        "run",
        &[
            "atlas run -- /bin/sh -c 'ls \"$HOME/atlas-work\"'",
            "atlas run --timeout 300 -- xcodebuild test-without-building -xctestrun Tests.xctestrun",
        ],
    ),
    (
        "job",
        &[
            "atlas job start --name web -- /usr/bin/python3 -m http.server 8000",
            "atlas job list",
            "atlas job log 7c2e91a04b3f",
            "atlas job stop 7c2e91a04b3f",
        ],
    ),
    (
        "collect",
        &[
            "atlas collect atlas-work/report.pdf",
            "atlas collect --max-size 200M atlas-work/build.zip",
        ],
    ),
    (
        "readiness",
        &[
            "atlas readiness --tool git",
            "atlas readiness --tool git --app Xcode",
        ],
    ),
    ("monitor", &["atlas monitor", "atlas monitor --port 8800"]),
    ("logout", &["atlas logout", "atlas logout --force"]),
    (
        "uninstall",
        &["atlas uninstall", "atlas uninstall --remove-helper"],
    ),
    (
        "agents",
        &[
            "atlas agents",
            "atlas agents install",
            "atlas agents project",
        ],
    ),
    (
        "agent-guide",
        &[
            "atlas agent-guide",
            "atlas agent-guide --skill",
            "atlas agent-guide --errors",
        ],
    ),
];

fn examples(name: &str) -> &'static [&'static str] {
    EXAMPLES
        .iter()
        .find(|(command, _)| *command == name)
        .map_or(&[], |(_, examples)| examples)
}

/// The command line with the grouped help.
pub fn command() -> clap::Command {
    let mut root = Cli::command();

    for (name, examples) in EXAMPLES {
        let text = format!("Examples:\n  {}", examples.join("\n  "));
        root = root.mut_subcommand(name, |command| command.after_help(text));
    }

    let mut groups = String::new();

    for (title, names) in GROUPS {
        groups.push_str(&format!("{title}:\n"));

        for name in *names {
            let about = root
                .find_subcommand(name)
                .and_then(|command| command.get_about())
                .map(|about| about.to_string())
                .unwrap_or_default();
            let first = about.lines().next().unwrap_or("").trim_end_matches('.');
            groups.push_str(&format!("  {name:<14} {first}\n"));
        }

        groups.push('\n');
    }

    root.help_template(format!(
        "{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n{groups}Global options:\n{{options}}\n\nRun atlas COMMAND --help for details. Without a terminal, every command prints JSON."
    ))
}

/// A command line that does not parse, said plainly with an example of
/// what works, as JSON when a program is reading. Help and --version print
/// as usual.
pub fn usage(error: clap::Error) -> ExitCode {
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        error.exit();
    }

    let arguments: Vec<_> = std::env::args_os().collect();

    // Retain Clap's partial matches for diagnostics, including option-value
    // ownership and aliases. Some validation errors have no Usage context.
    // Commands Atlas runs itself are never suggested.
    let mut root = Cli::command().ignore_errors(true);
    let matches = root.try_get_matches_from_mut(&arguments).ok();
    let mut current = &root;
    let mut matched = matches.as_ref();
    let mut path = Vec::new();

    while let Some((name, nested)) = matched.and_then(clap::ArgMatches::subcommand) {
        let Some(command) = current
            .find_subcommand(name)
            .filter(|command| !command.is_hide_set())
        else {
            break;
        };
        path.push(command.get_name().to_owned());
        current = command;
        matched = Some(nested);
    }

    let name = (!path.is_empty()).then(|| path.join(" "));
    let message = explain(&error, name.as_deref());

    // An example that uses the option that went wrong, when there is one.
    let option = match error.get(ContextKind::InvalidArg) {
        Some(ContextValue::String(value)) => value.split_whitespace().next().map(str::to_owned),
        _ => None,
    };

    let example = match path.as_slice() {
        [command] => {
            let list = examples(command);
            option
                .as_deref()
                .filter(|option| option.starts_with('-'))
                .and_then(|option| {
                    list.iter()
                        .find(|example| example.split_whitespace().any(|word| word == option))
                })
                .or(list.first())
                .copied()
        }
        _ => None,
    };

    let format = ui::detect(
        arguments
            .iter()
            .skip(1)
            .take_while(|argument| *argument != "--")
            .any(|argument| argument == "--json"),
        std::env::var("ATLAS_JSON").ok().as_deref(),
        std::io::stdout().is_terminal(),
    );
    ui::set(format);

    match format {
        Format::Json => {
            eprintln!(
                "{}",
                serde_json::json!({
                    "ok": false,
                    "code": "usage",
                    "error": message,
                    "next": ui::hints::next("usage", None),
                })
            );
        }
        Format::Human => {
            let help = match &name {
                Some(name) => format!("atlas {name} --help"),
                None => "atlas --help".to_owned(),
            };

            let fix = match example {
                Some(example) => {
                    format!("Try {}, or see {}.", style::cmd(example), style::cmd(help))
                }
                None => format!("See {}.", style::cmd(help)),
            };

            eprintln!("{} {message}\n  {} {fix}", style::fail(), style::dim("→"));
        }
    }

    ExitCode::from(2)
}

/// The choices of a required group as clap names them, `<--app <APP>|--all>`:
/// split on the `|` between choices, never on one inside a value name
/// (`--login <on|off|status>`).
fn choices(argument: &str) -> Option<Vec<&str>> {
    let inner = argument.strip_prefix('<')?.strip_suffix('>')?;
    let (mut depth, mut start, mut parts) = (0usize, 0, Vec::new());

    for (index, character) in inner.char_indices() {
        match character {
            '<' | '[' => depth += 1,
            '>' | ']' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                parts.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }

    parts.push(&inner[start..]);
    (parts.len() > 1).then_some(parts)
}

/// What was wrong with the command line, in a sentence.
fn explain(error: &clap::Error, name: Option<&str>) -> String {
    let context = |kind| match error.get(kind) {
        Some(ContextValue::String(value)) => Some(value.clone()),
        Some(ContextValue::Strings(values)) => Some(values.join(", ")),
        _ => None,
    };

    let list = |kind| match error.get(kind) {
        Some(ContextValue::Strings(values)) => values.clone(),
        Some(ContextValue::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    };

    // `--tool <NAME>` as `--tool`, `<TEXT>` as TEXT, and a choice of
    // several (`<--app <APP>|--all>`) as `--app or --all`.
    let one = |argument: &str| {
        let word = argument.split_whitespace().next().unwrap_or("");
        word.trim_matches(|c| c == '<' || c == '>' || c == '.' || c == '[' || c == ']')
            .to_owned()
    };

    let bare = |argument: String| match choices(&argument) {
        Some(choices) => choices
            .iter()
            .map(|choice| one(choice))
            .collect::<Vec<_>>()
            .join(" or "),
        None => one(&argument),
    };

    let command = name.map_or("atlas".to_owned(), |name| format!("atlas {name}"));
    let suggestion = |kind| {
        let suggested = list(kind);
        (!suggested.is_empty()).then(|| format!(" Did you mean {}?", suggested.join(" or ")))
    };
    let invalid = context(ContextKind::InvalidArg).map(bare);
    let value = context(ContextKind::InvalidValue);

    match error.kind() {
        ErrorKind::InvalidValue if value.as_deref() == Some("") => {
            format!("{} needs a value.", invalid.unwrap_or_default())
        }
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => {
            let valid = list(ContextKind::ValidValue);
            let mut text = format!(
                "\"{}\" does not work for {}.",
                value.unwrap_or_default(),
                invalid.unwrap_or_default()
            );

            if !valid.is_empty() {
                text.push_str(&format!(" Use {}.", valid.join(", ")));
            } else if error.kind() == ErrorKind::ValueValidation
                && let Some(reason) = std::error::Error::source(error)
            {
                // Atlas's own parsers say what works in a sentence.
                let reason = reason.to_string();
                if reason.ends_with('.') {
                    text.push_str(&format!(" {reason}"));
                }
            }

            text
        }
        ErrorKind::InvalidSubcommand => format!(
            "There is no command {}.{}",
            context(ContextKind::InvalidSubcommand).unwrap_or_default(),
            suggestion(ContextKind::SuggestedSubcommand).unwrap_or_default()
        ),
        ErrorKind::UnknownArgument => {
            let argument = context(ContextKind::InvalidArg).unwrap_or_default();
            if argument.starts_with('-') {
                format!(
                    "{command} has no option {argument}.{}",
                    suggestion(ContextKind::SuggestedArg).unwrap_or_default()
                )
            } else {
                format!("{command} does not take {argument}.")
            }
        }
        ErrorKind::MissingRequiredArgument => {
            let missing: Vec<String> = list(ContextKind::InvalidArg)
                .into_iter()
                .map(bare)
                .collect();
            format!("{command} needs {}.", missing.join(" and "))
        }
        ErrorKind::ArgumentConflict => format!(
            "{} cannot be used with {}.",
            invalid.unwrap_or_default(),
            context(ContextKind::PriorArg).map(bare).unwrap_or_default()
        ),
        ErrorKind::TooManyValues | ErrorKind::WrongNumberOfValues | ErrorKind::TooFewValues => {
            format!("{command} got the wrong number of values.")
        }
        _ => {
            // clap's own first line, without its "error: " prefix.
            let rendered = error.render().to_string();
            let line = rendered
                .lines()
                .next()
                .unwrap_or("")
                .trim_start_matches("error: ");
            let mut chars = line.chars();
            chars.next().map_or(String::new(), |first| {
                first.to_uppercase().collect::<String>() + chars.as_str() + "."
            })
        }
    }
}

/// Every visible command with its full help, as Markdown: the agent
/// plugin's commands reference (a test keeps the plugin's copy equal).
pub(super) fn command_reference() -> String {
    let mut text = String::from(
        "# Atlas commands\n\nGenerated from `atlas --help`. Every command prints \
         JSON when its output is not a terminal (or with `--json`) and exits non-zero on \
         failure.\n",
    );

    // With each command's examples, as `--help` shows them.
    let mut root = command().bin_name("atlas");
    root.build();

    // In the order of `atlas --help`, each command followed by its own
    // subcommands (such as environment rollback).
    for name in GROUPS.iter().flat_map(|(_, names)| names.iter()) {
        let Some(command) = root.find_subcommand_mut(name) else {
            continue;
        };

        reference_section(&mut text, &format!("atlas {name}"), command);

        let nested: Vec<String> = command
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
            .map(|sub| sub.get_name().to_owned())
            .collect();
        for sub in nested {
            if let Some(subcommand) = command.find_subcommand_mut(&sub) {
                reference_section(&mut text, &format!("atlas {name} {sub}"), subcommand);
            }
        }
    }

    text
}

/// One command's section of the reference: its long help, verbatim.
fn reference_section(text: &mut String, title: &str, command: &mut clap::Command) {
    let help = command.render_long_help().to_string();
    text.push_str(&format!(
        "\n## {title}\n\n```text\n{}\n```\n",
        help.trim_end()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent plugin's command reference is exactly this CLI's help.
    #[test]
    fn the_plugin_command_reference_matches_this_cli() {
        let checked_in = include_str!("../../agents/atlas/skills/atlas/references/commands.md");
        assert_eq!(
            checked_in,
            command_reference(),
            "regenerate with: atlas agent-guide --commands"
        );
    }

    /// Each `atlas …` the guide or the error reference shows in backticks
    /// names a visible command (and subcommand) and only options that
    /// command has.
    #[test]
    fn every_command_and_option_the_guide_shows_exists() {
        let mut root = command();
        root.build();

        let shown = [atlas::agent::GUIDE, atlas::agent::ERRORS];
        for span in shown
            .iter()
            .flat_map(|text| text.split('`').skip(1).step_by(2))
        {
            let Some(rest) = span.strip_prefix("atlas ") else {
                continue;
            };
            let mut words = rest
                .split_whitespace()
                .take_while(|word| *word != "--")
                .peekable();
            let name = words.next().unwrap();
            let mut command = root
                .find_subcommand(name)
                .filter(|command| !command.is_hide_set())
                .unwrap_or_else(|| panic!("`{span}`: no command {name}"));
            if let Some(sub) = words
                .peek()
                .and_then(|word| command.find_subcommand(word))
                .filter(|sub| !sub.is_hide_set())
            {
                command = sub;
                words.next();
            }

            for word in words
                .map(|word| word.trim_start_matches('['))
                .filter(|word| word.starts_with("--"))
            {
                let flag = word
                    .trim_start_matches("--")
                    .trim_end_matches(']')
                    .split('=')
                    .next()
                    .unwrap();
                let known = flag == "help"
                    || command
                        .get_arguments()
                        .chain(root.get_arguments())
                        .any(|arg| arg.get_long() == Some(flag) && !arg.is_hide_set());
                assert!(
                    known,
                    "`{span}`: {} has no option --{flag}",
                    command.get_name()
                );
            }
        }
    }

    /// Every key and pointer example the guide or the error reference shows
    /// parses as this CLI takes it, so a named click is never shown with a
    /// window check it cannot take. Placeholders become values, `[…]`
    /// groups are left out and input from a file is dropped.
    #[test]
    fn every_input_example_the_guide_shows_parses() {
        let shown = [atlas::agent::GUIDE, atlas::agent::ERRORS];
        let mut parsed = 0;
        for span in shown
            .iter()
            .flat_map(|text| text.split('`').skip(1).step_by(2))
        {
            let mut words: Vec<String> = Vec::new();
            let mut optional = false;
            for word in span
                .split(" < ")
                .next()
                .unwrap_or_default()
                .split_whitespace()
            {
                if word.starts_with('[') {
                    optional = !word.ends_with(']');
                    continue;
                }
                if optional {
                    optional = !word.ends_with(']');
                    continue;
                }
                words.push(match word.trim_matches('"') {
                    "X" => "100".to_owned(),
                    "Y" => "200".to_owned(),
                    "N" => "1".to_owned(),
                    other => other.to_owned(),
                });
            }
            let input = matches!(
                words.get(1).map(String::as_str),
                Some("type" | "key" | "scroll" | "drag" | "click")
            );
            if words.first().map(String::as_str) != Some("atlas")
                || !input
                || words[2..].iter().all(|word| word.starts_with('-'))
            {
                continue;
            }
            if let Err(error) = command().try_get_matches_from(&words) {
                panic!("`{span}` does not parse: {error}");
            }
            parsed += 1;
        }
        assert!(parsed >= 10, "{parsed}");
    }

    #[test]
    fn the_agent_guide_options_are_visible() {
        let help = command()
            .find_subcommand_mut("agent-guide")
            .unwrap()
            .render_long_help()
            .to_string();
        for option in ["--skill", "--commands", "--errors"] {
            assert!(help.contains(option), "{option}");
        }
    }

    #[test]
    fn every_visible_command_is_in_exactly_one_group() {
        let root = Cli::command();
        let visible: Vec<&str> = root
            .get_subcommands()
            .filter(|command| !command.is_hide_set())
            .map(|command| command.get_name())
            .collect();
        for name in &visible {
            let homes = GROUPS
                .iter()
                .filter(|(_, names)| names.contains(name))
                .count();
            assert_eq!(homes, 1, "{name} should be in one group");
        }

        for (_, names) in GROUPS {
            for name in *names {
                assert!(visible.contains(name), "{name} is not a visible command");
            }
        }
    }

    #[test]
    fn usage_errors_say_what_was_wrong_plainly() {
        let said = |line: &[&str]| {
            let error = command()
                .try_get_matches_from(std::iter::once("atlas").chain(line.iter().copied()))
                .expect_err("should not parse");
            explain(&error, Some(line[0]))
        };

        assert_eq!(said(&["readiness", "--tool"]), "--tool needs a value.");
        assert_eq!(
            said(&["update", "--forse"]),
            "atlas update has no option --forse. Did you mean --force?"
        );
        assert_eq!(said(&["type"]), "atlas type needs TEXT.");
        assert_eq!(said(&["close"]), "atlas close needs --app or --all.");
        assert_eq!(said(&["menubar", "--login"]), "--login needs a value.");
        assert_eq!(
            said(&["menubar", "--login", "maybe"]),
            "\"maybe\" does not work for --login. Use on, off, status."
        );
        assert_eq!(
            said(&["run", "--timeout", "601", "--", "true"]),
            "\"601\" does not work for --timeout. Give 1 to 600 seconds."
        );
        assert_eq!(said(&["drag", "1", "2"]), "atlas drag needs TO_X and TO_Y.");
        assert_eq!(
            said(&["click", "--button", "sideways", "1", "2"]),
            "\"sideways\" does not work for --button. Use left, middle, right."
        );
        assert!(said(&["scren"]).starts_with("There is no command scren."));
    }

    #[test]
    fn every_example_is_for_a_command_and_parses() {
        let root = command();

        for (name, examples) in EXAMPLES {
            assert!(root.find_subcommand(name).is_some(), "{name}");

            for example in *examples {
                let words = example
                    .split_whitespace()
                    .map(|word| word.trim_matches('"'));
                assert!(
                    command().try_get_matches_from(words).is_ok(),
                    "{example} should parse"
                );
            }
        }
    }

    #[test]
    fn the_help_shows_each_group_with_one_line_per_command() {
        let help = command().render_help().to_string();

        for (title, _) in GROUPS {
            assert!(help.contains(&format!("{title}:")), "{title}");
        }

        assert!(help.contains("  screenshot     Save a screenshot of the helper's screen, an app's windows or a region\n"));
        assert!(help.contains("Global options:"));
    }
}
