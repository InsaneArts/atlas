//! `atlas --help`: the commands in a few groups a person can scan,
//! built from each command's own one-line description so they never drift.
use super::cli::Cli;
use atlas_cli::ui::{self, Format, style};
use clap::{
    CommandFactory,
    error::{ContextKind, ContextValue, ErrorKind},
};
use std::{io::IsTerminal, process::ExitCode};

/// A visible command and a few ways to use it, shown under its `--help`
/// and in its usage errors.
type Entry = (&'static str, &'static [&'static str]);

/// Each visible command in exactly one group, in reading order.
const GROUPS: &[(&str, &[Entry])] = &[
    (
        "Get started",
        &[
            ("init", &[]),
            (
                "doctor",
                &[
                    "atlas doctor",
                    "atlas doctor --verbose",
                    "atlas doctor --login-timing",
                ],
            ),
            (
                "agents",
                &[
                    "atlas agents",
                    "atlas agents install",
                    "atlas agents project",
                ],
            ),
            ("menubar", &[]),
        ],
    ),
    (
        "Look at the helper",
        &[
            ("screenshot", &[]),
            ("windows", &["atlas windows --app TextEdit"]),
            (
                "elements",
                &["atlas elements --app TextEdit --role AXButton"],
            ),
            ("apps", &[]),
            ("status", &[]),
            ("activity", &["atlas activity", "atlas activity --all"]),
        ],
    ),
    (
        "Act in the helper",
        &[
            (
                "click",
                &[
                    "atlas click --app TextEdit --name Save",
                    "atlas click --app TextEdit --ref 8123abc.12",
                    "atlas click 1200 880",
                ],
            ),
            (
                "element",
                &[
                    "atlas element press --ref 8123abc.12",
                    "atlas element set-value --ref 8123abc.4 --value \"hello\"",
                    "atlas element pick --ref 8123abc.7 --option 8123abc.9",
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
        ],
    ),
    (
        "Prepare the helper",
        &[
            (
                "readiness",
                &[
                    "atlas readiness --tool git",
                    "atlas readiness --tool git --app Xcode",
                ],
            ),
            ("environment", &[]),
            (
                "install-app",
                &[
                    "atlas install-app ./build/Release/Probe.app",
                    "atlas open --app Probe",
                ],
            ),
            ("verify-native", &[]),
        ],
    ),
    (
        "Take over or recover",
        &[
            ("switch", &[]),
            (
                "monitor",
                &[
                    "atlas monitor",
                    "atlas monitor --terminal",
                    "atlas monitor --terminal --read-only",
                    "atlas monitor --port 8800",
                ],
            ),
            ("restart", &[]),
            ("logout", &["atlas logout", "atlas logout --force"]),
            ("start-input", &[]),
        ],
    ),
    (
        "Look after Atlas",
        &[
            ("update", &[]),
            (
                "uninstall",
                &["atlas uninstall", "atlas uninstall --remove-helper"],
            ),
            ("install", &[]),
            ("setup", &[]),
            ("config", &[]),
            (
                "agent-guide",
                &[
                    "atlas agent-guide",
                    "atlas agent-guide --skill",
                    "atlas agent-guide --errors",
                ],
            ),
        ],
    ),
];

fn entries() -> impl Iterator<Item = &'static Entry> {
    GROUPS.iter().flat_map(|(_, commands)| commands.iter())
}

fn examples(name: &str) -> &'static [&'static str] {
    entries()
        .find(|(command, _)| *command == name)
        .map_or(&[], |(_, examples)| examples)
}

/// The command line with the grouped help.
pub(super) fn command() -> clap::Command {
    let mut root = Cli::command();

    for (name, examples) in entries().filter(|(_, examples)| !examples.is_empty()) {
        let text = format!("Examples:\n  {}", examples.join("\n  "));
        root = root.mut_subcommand(name, |command| command.after_help(text));
    }

    let mut groups = String::new();

    for (title, commands) in GROUPS {
        groups.push_str(&format!("{title}:\n"));

        for (name, _) in *commands {
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
pub(super) fn usage(error: clap::Error) -> ExitCode {
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        error.exit();
    }

    let arguments: Vec<_> = std::env::args_os().collect();
    let path = visible_path(&arguments);
    let name = (!path.is_empty()).then(|| path.join(" "));
    let message = explain(&error, name.as_deref());
    let example = example_for(&path, &error);

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
                    "next": atlas_core::error::advice::next(atlas_core::error::Code::Usage, None),
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

/// The visible commands `arguments` name, as clap's partial matches give
/// them, including option-value ownership and aliases. Some validation
/// errors have no Usage context. Commands Atlas runs itself are never
/// suggested.
fn visible_path(arguments: &[std::ffi::OsString]) -> Vec<String> {
    let mut root = Cli::command().ignore_errors(true);
    let matches = root.try_get_matches_from_mut(arguments).ok();
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

    path
}

/// An example of the command at `path`: one that uses the option that went
/// wrong, when there is one.
fn example_for(path: &[String], error: &clap::Error) -> Option<&'static str> {
    let [command] = path else {
        return None;
    };
    let option = match error.get(ContextKind::InvalidArg) {
        Some(ContextValue::String(value)) => value.split_whitespace().next(),
        _ => None,
    };
    let list = examples(command);

    option
        .filter(|option| option.starts_with('-'))
        .and_then(|option| {
            list.iter()
                .find(|example| example.split_whitespace().any(|word| word == option))
        })
        .or(list.first())
        .copied()
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
    for (name, _) in entries() {
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
mod tests;
