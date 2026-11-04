use super::*;

#[test]
fn terminal_monitor_is_explicit_and_read_only_requires_the_terminal() {
    assert!(matches!(
        parse(&["monitor"]).unwrap().command,
        Some(Command::Monitor {
            terminal: false,
            read_only: false,
            ..
        })
    ));
    assert!(matches!(
        parse(&["monitor", "--terminal", "--read-only"])
            .unwrap()
            .command,
        Some(Command::Monitor {
            terminal: true,
            read_only: true,
            ..
        })
    ));
    assert!(parse(&["monitor", "--read-only"]).is_err());
    assert!(parse(&["monitor", "--terminal", "--json"]).is_err());
    assert!(parse(&["monitor", "--terminal", "--port", "8765"]).is_err());
    assert!(parse(&["monitor", "--max-dimension", "640"]).is_err());
    assert!(parse(&["monitor", "--terminal", "--max-dimension", "319"]).is_err());
    assert!(parse(&["monitor", "--terminal", "--max-dimension", "4097"]).is_err());
    assert!(parse(&["monitor", "--terminal", "--max-dimension", "4096"]).is_ok());
    assert!(parse(&["monitor", "--terminal", "--capture-backend", "sck"]).is_ok());
    assert!(parse(&["monitor", "--capture-backend", "sck"]).is_err());
    assert!(parse(&["monitor", "--terminal", "--capture-backend", "unknown"]).is_err());
    assert!(matches!(
        parse(&["monitor", "--terminal", "--max-dimension", "640"])
            .unwrap()
            .command,
        Some(Command::Monitor {
            max_dimension: Some(640),
            ..
        })
    ));
}

#[test]
fn allowing_disruption_is_separate_from_reinstalling_an_older_release() {
    let cli = parse(&["update", "--force"]).unwrap();
    assert!(!cli.allow_disrupt);
    assert!(matches!(
        cli.command,
        Some(Command::Update(Update { force: true, .. }))
    ));

    let cli = parse(&["update", "--allow-disrupt"]).unwrap();
    assert!(cli.allow_disrupt);
    assert!(matches!(
        cli.command,
        Some(Command::Update(Update { force: false, .. }))
    ));

    for command in ["restart", "start-input", "init", "setup", "update-finish"] {
        assert!(parse(&[command, "--allow-disrupt"]).unwrap().allow_disrupt);
    }
}

fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("atlas").chain(args.iter().copied()))
}

#[test]
fn jobs_take_their_command_after_two_dashes() {
    let Some(Command::Job {
        command:
            JobCommand::Start {
                name,
                timeout,
                argv,
            },
    }) = parse(&["job", "start", "--", "/bin/sh", "-c", "echo --name"])
        .unwrap()
        .command
    else {
        panic!("a job start");
    };
    assert_eq!(argv, ["/bin/sh", "-c", "echo --name"]);
    assert_eq!((name, timeout), (None, 3600));
    assert!(parse(&["job", "start", "/bin/sleep"]).is_err());
}

#[test]
fn job_ids_sizes_and_times_are_checked_when_parsed() {
    for args in [
        &["job", "stop", "7c2e91a04b3"][..],
        &["job", "stop", "7C2E91A04B3F"],
        &["job", "log", "nope"],
        &["job", "stop", "7c2e91a04b3f", "--grace", "0"],
        &["job", "stop", "7c2e91a04b3f", "--grace", "61"],
        &["job", "stop", "7c2e91a04b3f", "--task", "ABC"],
        &["job", "log", "7c2e91a04b3f", "--bytes", "1048577"],
        &["job", "start", "--timeout", "0", "--", "x"],
        &["job", "start", "--timeout", "86401", "--", "x"],
        &["job", "start", "--name", "", "--", "x"],
        &["collect", "--max-size", "2G", "a"],
        &["collect", "--max-size", "0", "a"],
        &["collect", "--max-size", "12Q", "a"],
    ] {
        assert!(parse(args).is_err(), "{args:?}");
    }
    assert_eq!(file_size("200M"), Ok(200 << 20));
    assert_eq!(file_size("1G"), Ok(1 << 30));
    assert_eq!(file_size("512k"), Ok(512 << 10));
    assert_eq!(file_size("17"), Ok(17));
    assert!(parse(&["job", "start", "--timeout", "86400", "--", "x"]).is_ok());
    assert_eq!(
        job_id("x").unwrap_err(),
        "Give the job's id, as atlas job start printed it."
    );
}

#[test]
fn collect_takes_one_to_sixteen_files() {
    assert!(parse(&["collect"]).is_err());
    let sixteen: Vec<String> = (0..16).map(|n| format!("f{n}")).collect();
    let mut args = vec!["collect"];
    args.extend(sixteen.iter().map(String::as_str));
    let Some(Command::Collect { files, max_size }) = parse(&args).unwrap().command else {
        panic!("a collection");
    };
    assert_eq!((files.len(), max_size), (16, 64 << 20));
    args.push("f16");
    assert!(parse(&args).is_err());
}

#[test]
fn setup_workers_require_an_explicit_account_identity() {
    let request = r#"{"action":"prepare","uid":502,"controller":"controller"}"#;
    assert_eq!(
        parse(&["setup-helper", request]).err().unwrap().kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
    let identity =
        r#"{"uid":502,"name":"helper","generatedUid":"6DB4B7AF-BE3C-486C-BD46-1B4558946919"}"#;
    assert!(parse(&["setup-helper", request, "--account", identity]).is_ok());
}

#[test]
fn activity_takes_all_or_a_session_key_and_a_limit_from_one_to_fifty() {
    let activity = |args: &[&str]| match parse(args) {
        Ok(Cli {
            command:
                Some(Command::Activity(Activity {
                    all,
                    session,
                    limit,
                })),
            ..
        }) => Some((all, session.map(String::from), limit)),
        _ => None,
    };

    assert_eq!(activity(&["activity"]), Some((false, None, 10)));
    assert_eq!(activity(&["activity", "--all"]), Some((true, None, 10)));
    assert_eq!(
        activity(&["activity", "--session", "3f9a1c07b2e4", "--limit", "50"]),
        Some((false, Some("3f9a1c07b2e4".into()), 50))
    );
    assert_eq!(
        activity(&["activity", "--limit", "1"]),
        Some((false, None, 1))
    );
    for refused in [
        &["activity", "--all", "--session", "3f9a1c07b2e4"][..],
        &["activity", "--session", "3F9A1C07B2E4"],
        &["activity", "--session", "3f9a1c07b2e"],
        &["activity", "--session", "../sessions"],
        &["activity", "--limit", "0"],
        &["activity", "--limit", "51"],
    ] {
        assert!(parse(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn cleanup_takes_apply_session_and_task_and_install_app_a_path() {
    let cleanup = |args: &[&str]| match parse(args) {
        Ok(Cli {
            command:
                Some(Command::Cleanup(Cleanup {
                    apply,
                    session,
                    task,
                })),
            ..
        }) => Some((apply, session.map(String::from), task.map(String::from))),
        _ => None,
    };

    assert_eq!(cleanup(&["cleanup"]), Some((false, None, None)));
    assert_eq!(cleanup(&["cleanup", "--apply"]), Some((true, None, None)));
    assert_eq!(
        cleanup(&[
            "cleanup",
            "--session",
            "3f9a1c07b2e4",
            "--task",
            "5be0d1a2c3f4",
            "--apply"
        ]),
        Some((
            true,
            Some("3f9a1c07b2e4".into()),
            Some("5be0d1a2c3f4".into())
        ))
    );
    assert!(matches!(
        parse(&["install-app", "./build/Probe.app"]),
        Ok(Cli {
            command: Some(Command::InstallApp { path }),
            ..
        }) if path == std::path::Path::new("./build/Probe.app")
    ));
    assert!(parse(&["install-app"]).is_err(), "a path is needed");

    for refused in [
        &["cleanup", "--session", "3F9A1C07B2E4"][..],
        &["cleanup", "--task", "../tasks"],
        &["cleanup", "notes.rtf"],
    ] {
        assert!(parse(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn clicks_take_coordinates_or_an_element_never_both() {
    assert!(parse(&["click", "10", "20"]).is_ok());
    assert!(parse(&["click", "--app", "Calculator", "--name", "7"]).is_ok());
    assert!(parse(&["click", "--pid", "42", "--role", "AXButton"]).is_ok());
    assert!(parse(&["click", "10", "20", "--name", "Save"]).is_err());
    assert!(parse(&["click", "10", "20", "--role", "AXButton"]).is_err());
    assert!(parse(&["click", "10"]).is_err(), "Y is required with X");
    assert!(parse(&["click", "--app", "A", "--pid", "1", "--name", "x"]).is_err());
    assert!(parse(&["click", "10", "20", "--app", "A"]).is_ok());
    assert!(parse(&["click", "10", "20", "--pid", "1"]).is_ok());
    assert!(parse(&["click", "10", "20", "--app", "A", "--pid", "1"]).is_err());
}

#[test]
fn field_is_accepted_by_type_and_key_only() {
    for accepted in [
        &["type", "--app", "A", "--field", "Email", "hi"][..],
        &["type", "hi", "--window", "4120", "--field", "Email"],
        &["key", "--window", "4120", "--field", "Email", "tab"],
    ] {
        assert!(parse(accepted).is_ok(), "{accepted:?}");
    }
    for refused in [
        &["click", "1", "2", "--field", "Email"][..],
        &["scroll", "1", "2", "--dy", "1", "--field", "Email"],
        &["drag", "1", "2", "3", "4", "--field", "Email"],
        &["input", r#"[{"type":"key","key":"a"}]"#, "--field", "Email"],
        &["move", "1", "2", "--field", "Email"],
    ] {
        assert!(parse(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn a_window_is_named_on_keys_and_on_pointer_actions_at_a_point() {
    let window = |args: &[&str]| match parse(args) {
        Ok(Cli {
            command:
                Some(
                    Command::Type(Type { window, .. })
                    | Command::Key(Keys { window, .. })
                    | Command::Click(Click { window, .. })
                    | Command::Scroll(Scroll { window, .. })
                    | Command::Drag(Drag { window, .. }),
                ),
            ..
        }) => window,
        _ => None,
    };

    assert_eq!(window(&["type", "--window", "4120", "hi"]), Some(4120));
    assert_eq!(window(&["key", "cmd+s", "--window", "4120"]), Some(4120));
    assert_eq!(window(&["click", "1", "2", "--window", "4120"]), Some(4120));
    assert_eq!(
        window(&["scroll", "1", "2", "--dy", "1", "--window", "4120"]),
        Some(4120)
    );
    assert_eq!(
        window(&["drag", "1", "2", "3", "4", "--window", "4120"]),
        Some(4120)
    );
    assert!(
        parse(&["click", "--app", "A", "--name", "Save", "--window", "4120"]).is_err(),
        "a click by name checks its own element's window"
    );
    assert!(parse(&["input", "[]", "--window", "4120"]).is_err());
    assert!(parse(&["type", "--window", "x", "hi"]).is_err());
}

#[test]
fn keyboard_and_pointer_commands_take_an_app() {
    let app = |args: &[&str]| match parse(args) {
        Ok(Cli {
            command:
                Some(
                    Command::Type(Type { target, .. })
                    | Command::Key(Keys { target, .. })
                    | Command::Scroll(Scroll { target, .. })
                    | Command::Drag(Drag { target, .. })
                    | Command::Input { target, .. },
                ),
            ..
        }) => Some((target.app, target.pid)),
        _ => None,
    };
    let named = Some((Some("A".to_owned()), None));

    assert_eq!(app(&["type", "--app", "A", "hi"]), named);
    assert_eq!(app(&["type", "hi", "--app", "A"]), named);
    assert_eq!(app(&["key", "--pid", "3", "escape"]), Some((None, Some(3))));
    assert_eq!(app(&["scroll", "1", "2", "--dy", "3", "--app", "A"]), named);
    assert_eq!(app(&["drag", "1", "2", "3", "4", "--app", "A"]), named);
    assert_eq!(
        app(&["input", r#"[{"type":"key","key":"a"}]"#, "--app", "A"]),
        named
    );
    assert!(matches!(
        parse(&["key", "cmd+s", "--app", "A"]),
        Ok(Cli { command: Some(Command::Key(Keys { keys, .. })), .. }) if keys == ["cmd+s"]
    ));
    assert!(parse(&["type", "hi", "--app", "A", "--pid", "1"]).is_err());
    assert!(parse(&["move", "1", "2", "--app", "A"]).is_err());
}

#[test]
fn screenshots_take_an_app_or_a_region_never_both() {
    let given = |args: &[&str]| match parse(args) {
        Ok(Cli {
            command:
                Some(Command::Screenshot(Screenshot {
                    target,
                    region,
                    max_dimension,
                    ..
                })),
            ..
        }) => Ok((target.app, target.pid, region, max_dimension)),
        Err(error) => Err(error.to_string()),
        _ => Err("not a screenshot".into()),
    };
    let region = |x, y, width, height| Region {
        x,
        y,
        width,
        height,
    };

    assert_eq!(given(&["screenshot"]), Ok((None, None, None, None)));
    assert_eq!(
        given(&["screenshot", "--app", "TextEdit", "--max-dimension", "400"]),
        Ok((Some("TextEdit".into()), None, None, Some(400)))
    );
    assert_eq!(
        given(&["capture", "--pid", "42"]),
        Ok((None, Some(42), None, None))
    );
    assert_eq!(
        given(&["screenshot", "--region", "-50,-50,200,200"]),
        Ok((None, None, Some(region(-50, -50, 200, 200)), None))
    );
    assert_eq!(
        given(&["screenshot", "--region", "1.5, 2.25, 3, 4"]),
        Ok((None, None, Some(region(1, 2, 4, 5)), None))
    );

    for (args, says) in [
        (
            &["screenshot", "--app", "A", "--region", "0,0,10,10"][..],
            "cannot be used with",
        ),
        (
            &["screenshot", "--pid", "1", "--region", "0,0,10,10"],
            "cannot be used with",
        ),
        (
            &["screenshot", "--app", "A", "--pid", "1"],
            "cannot be used with",
        ),
        (&["screenshot", "--region", "0,0,10"], "X,Y,W,H"),
        (&["screenshot", "--region", "a,b,c,d"], "X,Y,W,H"),
        (&["screenshot", "--region", "0,0,inf,10"], "X,Y,W,H"),
        (&["screenshot", "--region", "0,0,0,10"], "more than 0"),
        (&["screenshot", "--region", "0,0,10,-5"], "more than 0"),
        (&["screenshot", "--region", "0,0,16385,10"], "16384"),
        (&["screenshot", "--region", "-16385,0,10,10"], "16384"),
    ] {
        let refused = given(args).unwrap_err();
        assert!(refused.contains(says), "{args:?}: {refused}");
    }
}

#[test]
fn a_screenshot_is_smaller_by_default_and_full_size_on_request() {
    use crate::dispatch::screenshot_size;
    use atlas_core::screen::frame::Size;

    assert_eq!(screenshot_size(None), Size::Standard);
    assert_eq!(screenshot_size(Some(0)), Size::Full);
    assert_eq!(screenshot_size(Some(800)), Size::Longest(800));
    assert_eq!(screenshot_size(Some(1456)), Size::Longest(1456));

    let given = |args: &[&str]| match parse(args) {
        Ok(Cli {
            command:
                Some(Command::Screenshot(Screenshot {
                    max_dimension,
                    new_file,
                    ..
                })),
            ..
        }) => Some((max_dimension, new_file)),
        _ => None,
    };
    assert_eq!(given(&["screenshot"]), Some((None, false)));
    assert_eq!(
        given(&["screenshot", "--max-dimension", "0"]),
        Some((Some(0), false))
    );
    assert_eq!(
        given(&["screenshot", "--max-dimension", "8192", "--new-file"]),
        Some((Some(8192), true))
    );
    assert_eq!(given(&["screenshot", "--max-dimension", "8193"]), None);
    assert_eq!(given(&["screenshot", "--max-dimension", "-1"]), None);
}

#[test]
fn the_menu_bar_serves_or_renders() {
    let Ok(Cli {
        command: Some(Command::Menubar(Menubar { port, render, .. })),
        ..
    }) = parse(&["menubar", "--render", "out"])
    else {
        panic!("menubar did not parse");
    };

    assert_eq!((port, render), (8765, Some(PathBuf::from("out"))));
    assert!(parse(&["menubar", "--port", "9000"]).is_ok());

    use atlas_cli::menubar::Login;
    for (state, expected) in [
        ("on", Login::On),
        ("off", Login::Off),
        ("status", Login::Status),
    ] {
        let Ok(Cli {
            command: Some(Command::Menubar(Menubar { login, .. })),
            ..
        }) = parse(&["menubar", "--login", state])
        else {
            panic!("menubar --login {state} did not parse");
        };
        assert_eq!(login, Some(expected));
    }
}

#[test]
fn scrolling_accepts_negative_lines() {
    let Ok(Cli {
        command: Some(Command::Scroll(Scroll { dy, .. })),
        ..
    }) = parse(&["scroll", "1", "2", "--dy", "-3"])
    else {
        panic!("scroll did not parse");
    };

    assert_eq!(dy, -3);
}

/// Text and keys that begin with a hyphen are typed and pressed, not
/// read as options.

#[test]
fn text_may_begin_with_a_hyphen_and_keys_may_be_one() {
    let Ok(Cli {
        command: Some(Command::Type(Type { text, stdin, .. })),
        ..
    }) = parse(&["type", "-hi there"])
    else {
        panic!("type did not parse");
    };

    assert_eq!((text.as_deref(), stdin), (Some("-hi there"), false));

    let Ok(Cli {
        command: Some(Command::Key(Keys { keys, .. })),
        ..
    }) = parse(&["key", "-", "cmd+-"])
    else {
        panic!("key did not parse");
    };

    assert_eq!(keys, ["-", "cmd+-"]);
    assert!(
        matches!(
            parse(&["key", "cmd+s", "--json"]),
            Ok(Cli {
                json: true,
                command: Some(Command::Key(_)),
                ..
            })
        ),
        "an option after the keys stays an option"
    );
    assert!(matches!(
        parse(&["type", "--stdin"]),
        Ok(Cli {
            command: Some(Command::Type(Type {
                text: None,
                stdin: true,
                ..
            })),
            ..
        })
    ));
    assert!(parse(&["type", "--help"]).is_err(), "help still shows");
}

#[test]
fn a_wait_lasts_at_most_a_day() {
    assert!(parse(&["wait", "--app", "A", "--timeout", "0"]).is_ok());
    assert!(parse(&["wait", "--app", "A", "--timeout", "86400"]).is_ok());
    assert!(parse(&["wait", "--app", "A", "--timeout", "86401"]).is_err());
    assert!(parse(&["wait", "--app", "A", "--timeout", "18446744073709551615"]).is_err());
}

#[test]
fn the_wait_is_whole_seconds_from_zero_to_six_hundred() {
    let wait = |given: &str| parse(&["--wait", given, "key", "escape"]).map(|cli| cli.wait);
    assert_eq!(wait("0").unwrap(), Some(Duration::ZERO));
    assert_eq!(wait("600").unwrap(), Some(CONTROL_WAIT_MAX));
    for refused in ["601", "-1", "1.5", "soon"] {
        assert!(wait(refused).is_err(), "{refused}");
    }
    assert_eq!(parse(&["key", "escape"]).unwrap().wait, None);
}

#[test]
fn the_flag_wins_over_the_variable_and_either_over_the_default() {
    let given = |flag: Option<u64>, variable: Option<&str>| {
        control_wait_given(flag.map(Duration::from_secs), variable.map(OsStr::new))
    };
    assert_eq!(given(None, None).unwrap(), CONTROL_WAIT);
    assert_eq!(given(None, Some("12")).unwrap(), Duration::from_secs(12));
    assert_eq!(given(Some(5), Some("12")).unwrap(), Duration::from_secs(5));
    assert_eq!(
        given(Some(5), Some("soon")).unwrap(),
        Duration::from_secs(5)
    );
    for refused in ["601", "-1", "1.5", "soon", ""] {
        let error = given(None, Some(refused)).unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
    }
}

#[test]
fn the_wait_flag_is_global_and_hidden() {
    for args in [
        &["type", "--wait", "5", "hi"][..],
        &["--wait", "5", "type", "hi"],
    ] {
        assert_eq!(
            parse(args).unwrap().wait,
            Some(Duration::from_secs(5)),
            "{args:?}"
        );
    }

    let help = match parse(&["type", "--help"]) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("help is shown"),
    };
    assert!(help.contains("--json"), "{help}");
    assert!(!help.contains("--wait"), "{help}");
}

/// Each action on an element is the one batch action of its word, its
/// value or option given only to the action that takes one.
#[test]
fn an_element_action_becomes_its_one_input_action() {
    let action = |args: &[&str]| match parse(args) {
        Ok(Cli {
            command: Some(Command::Element(element)),
            ..
        }) => element.as_input().map_err(|error| error.code),
        other => panic!("{args:?}: {:?}", other.err()),
    };

    for verb in [
        "press",
        "check",
        "uncheck",
        "expand",
        "collapse",
        "scroll-into-view",
    ] {
        assert_eq!(
            action(&["element", verb, "--ref", "8123abc.12", "--app", "A"]).unwrap(),
            serde_json::json!({"type": verb, "ref": "8123abc.12"})
        );
    }
    assert_eq!(
        action(&[
            "element",
            "set-value",
            "--ref",
            "8123abc.12",
            "--value",
            "-3"
        ])
        .unwrap(),
        serde_json::json!({"type": "set-value", "ref": "8123abc.12", "value": "-3"})
    );
    assert_eq!(
        action(&[
            "element",
            "pick",
            "--ref",
            "8123abc.12",
            "--option",
            "8123abc.14"
        ])
        .unwrap(),
        serde_json::json!({"type": "pick", "ref": "8123abc.12", "option": "8123abc.14"})
    );
    for misplaced in [
        &["element", "press", "--ref", "8123abc.12", "--value", "x"][..],
        &[
            "element",
            "check",
            "--ref",
            "8123abc.12",
            "--option",
            "8123abc.14",
        ],
        &[
            "element",
            "set-value",
            "--ref",
            "1a",
            "--value",
            "x",
            "--option",
            "1b",
        ],
    ] {
        assert_eq!(
            action(misplaced).unwrap_err(),
            atlas_core::error::Code::Usage,
            "{misplaced:?}"
        );
    }

    for refused in [
        &["element", "press"][..],
        &["element", "toggle", "--ref", "8123abc.12"],
        &["element", "set-value", "--ref", "8123abc.12"],
        &["element", "pick", "--ref", "8123abc.12"],
        &[
            "element",
            "press",
            "--ref",
            "8123abc.12",
            "--app",
            "A",
            "--pid",
            "1",
        ],
        &["element", "press", "--ref", "8123abc.12", "--window", "4"],
    ] {
        assert!(parse(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn an_environment_command_takes_the_same_time_limit_as_run() {
    let given = |args: &[&str]| match parse(args) {
        Ok(Cli {
            command:
                Some(Command::Environment(Environment {
                    command: Some(EnvironmentCommand::Run(Run { timeout, .. })),
                    ..
                })),
            ..
        }) => Some(timeout),
        _ => None,
    };

    assert_eq!(given(&["environment", "run", "--", "tool"]), Some(20));
    assert_eq!(
        given(&["environment", "run", "--timeout", "600", "--", "tool"]),
        Some(600)
    );
    assert_eq!(
        given(&["environment", "run", "--timeout", "0", "--", "tool"]),
        None
    );
    assert_eq!(
        given(&["environment", "run", "--timeout", "601", "--", "tool"]),
        None
    );
}
