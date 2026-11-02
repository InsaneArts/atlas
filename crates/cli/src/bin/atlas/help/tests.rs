use super::*;

/// The agent plugin's command reference is exactly this CLI's help.
#[test]
fn the_plugin_command_reference_matches_this_cli() {
    let checked_in =
        include_str!("../../../../../../agents/atlas/skills/atlas/references/commands.md");
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

    let shown = [atlas_client::agent::GUIDE, atlas_client::agent::ERRORS];
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
    let shown = [atlas_client::agent::GUIDE, atlas_client::agent::ERRORS];
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
            .filter(|(_, commands)| commands.iter().any(|(command, _)| command == name))
            .count();
        assert_eq!(homes, 1, "{name} should be in one group");
    }

    for (name, _) in entries() {
        assert!(visible.contains(name), "{name} is not a visible command");
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

    for (name, examples) in entries() {
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

    assert!(help.contains(
        "  screenshot     Save a screenshot of the helper's screen, an app's windows or a region\n"
    ));
    assert!(help.contains("Global options:"));
}
