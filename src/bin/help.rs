//! `understudy --help`: the commands in a few groups a person can scan,
//! built from each command's own one-line description so they never drift.
use super::Cli;
use clap::{
    CommandFactory,
    error::{ContextKind, ContextValue, ErrorKind},
};
use std::{io::IsTerminal, process::ExitCode};
use understudy::ui::{self, Format, style};

/// Each visible command in exactly one group, in reading order.
pub const GROUPS: &[(&str, &[&str])] = &[
    ("Get started", &["init", "doctor", "agents", "menubar"]),
    (
        "Look at the helper",
        &["screenshot", "windows", "elements", "apps", "status"],
    ),
    (
        "Act in the helper",
        &[
            "click", "type", "key", "move", "drag", "scroll", "input", "open", "close", "wait",
            "run",
        ],
    ),
    (
        "Prepare the helper",
        &["readiness", "environment", "verify-native"],
    ),
    (
        "Take over or recover",
        &["switch", "monitor", "restart", "start-input"],
    ),
    (
        "Look after Understudy",
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
        &["understudy doctor", "understudy doctor --verbose"],
    ),
    ("windows", &["understudy windows --app TextEdit"]),
    (
        "elements",
        &["understudy elements --app TextEdit --role AXButton"],
    ),
    (
        "click",
        &[
            "understudy click --app TextEdit --name Save",
            "understudy click 1200 880",
        ],
    ),
    ("type", &["understudy type \"hello\""]),
    (
        "key",
        &["understudy key cmd+s", "understudy key escape return"],
    ),
    ("move", &["understudy move 640 400"]),
    ("drag", &["understudy drag 100 200 400 200"]),
    ("scroll", &["understudy scroll 640 400 --dy 3"]),
    ("input", &["understudy input @actions.json"]),
    (
        "open",
        &[
            "understudy open --app TextEdit",
            "understudy open ./build/Debug/MyApp.app --env DEBUG=1",
            "understudy open https://example.com",
        ],
    ),
    ("close", &["understudy close --app TextEdit"]),
    (
        "wait",
        &[
            "understudy wait --app TextEdit",
            "understudy wait --title Save --gone",
        ],
    ),
    ("run", &["understudy run -- ls ~/Documents"]),
    (
        "readiness",
        &[
            "understudy readiness --tool git",
            "understudy readiness --tool git --app Xcode",
        ],
    ),
    (
        "monitor",
        &["understudy monitor", "understudy monitor --port 8800"],
    ),
    (
        "agents",
        &["understudy agents", "understudy agents install"],
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
        "{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n{groups}Global options:\n{{options}}\n\nRun understudy COMMAND --help for details. Without a terminal, every command prints JSON."
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
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let root = Cli::command();
    let name = arguments
        .iter()
        .take_while(|argument| argument.as_str() != "--")
        .find_map(|argument| {
            root.get_subcommands()
                .find(|command| {
                    command.get_name() == argument
                        || command.get_all_aliases().any(|alias| alias == argument)
                })
                .map(|command| command.get_name().to_owned())
        });
    let message = explain(&error, name.as_deref());
    let format = ui::detect(
        arguments.iter().any(|argument| argument == "--json"),
        std::env::var("UNDERSTUDY_JSON").ok().as_deref(),
        std::io::stdout().is_terminal(),
    );
    ui::set(format);
    match format {
        Format::Json => {
            eprintln!(
                "{}",
                serde_json::json!({"ok": false, "code": "usage", "error": message})
            );
        }
        Format::Human => {
            let help = match &name {
                Some(name) => format!("understudy {name} --help"),
                None => "understudy --help".to_owned(),
            };
            let fix = match name.as_deref().map(examples).and_then(|list| list.first()) {
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
    // `--tool <NAME>` as `--tool`, `<TEXT>` as TEXT.
    let bare = |argument: String| {
        let word = argument.split_whitespace().next().unwrap_or("").to_owned();
        word.trim_matches(|c| c == '<' || c == '>' || c == '.' || c == '[' || c == ']')
            .to_owned()
    };
    let command = name.map_or("understudy".to_owned(), |name| format!("understudy {name}"));
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

#[cfg(test)]
mod tests {
    use super::*;

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
                .try_get_matches_from(std::iter::once("understudy").chain(line.iter().copied()))
                .expect_err("should not parse");
            explain(&error, Some(line[0]))
        };
        assert_eq!(said(&["readiness", "--tool"]), "--tool needs a value.");
        assert_eq!(
            said(&["update", "--forse"]),
            "understudy update has no option --forse. Did you mean --force?"
        );
        assert_eq!(said(&["type"]), "understudy type needs TEXT.");
        assert_eq!(
            said(&["drag", "1", "2"]),
            "understudy drag needs TO_X and TO_Y."
        );
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
        assert!(help.contains("  screenshot     Save a screenshot of the helper's screen\n"));
        assert!(help.contains("Global options:"));
    }
}
