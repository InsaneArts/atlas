mod cli;
mod dispatch;
mod doctor;
mod help;
mod output;
mod policy;
mod prompt;
mod uninstall;
mod update;
mod user;

use atlas_cli::ui::{self, Format, render, style};
use atlas_control::interrupt::{Interrupt, Stopped};
use atlas_core::{
    config::Config,
    error::{Code, Error},
};
use clap::FromArgMatches;
use cli::{Cli, Command};
use nix::sys::signal::Signal;
use policy::Policy;
use serde_json::json;
use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
    process::ExitCode,
};

/// The terminal attaches to the installed helper by default, even when run
/// from a development checkout with settings of its own. Clap has already
/// resolved an explicit --config or ATLAS_CONFIG here.
fn configuration(command: Option<&Command>, given: Option<PathBuf>) -> PathBuf {
    if given.is_none()
        && matches!(command, Some(Command::Monitor { terminal: true, .. }))
        && let Ok(installed) = atlas_core::config::installed_path()
    {
        return installed;
    }
    atlas_core::config::effective(given)
}

fn main() -> ExitCode {
    let matches = match help::command().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => return help::usage(error),
    };

    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    let config = configuration(cli.command.as_ref(), cli.config.take());

    // `atlas` alone shows what it can do; opened as an app (Finder,
    // Launchpad, `open`), it shows the window instead. launchd names the
    // job that started it: the helper's one-time opener opens the window
    // once the desktop is up.
    let Some(command) = cli.command.take() else {
        if opened_as_app() {
            if let Some(opened) = crate::user::open::first_login_opener() {
                return opened;
            }
            return open_app(&config);
        }
        let _ = help::command().print_help();
        return ExitCode::SUCCESS;
    };

    let policy = Policy::of(&command);

    // A proof worker reads only its private, parent-owned standard input. It
    // needs no client, controller claim, signal task or activity attribution.
    if matches!(command, Command::SessionProofServe) {
        return match atlas_daemon::daemon::proof::serve_blocking() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{}", json!(error));
                ExitCode::FAILURE
            }
        };
    }

    // Commands run by Atlas itself always answer in JSON, and their
    // failures carry no step for an agent: another Atlas process reads them.
    let machine = policy.machine_only();
    let format = if machine {
        Format::Json
    } else {
        ui::detect(
            cli.json,
            std::env::var("ATLAS_JSON").ok().as_deref(),
            std::io::stdout().is_terminal(),
        )
    };

    ui::set(format);

    // A settings file is never read from Atlas's own folder, whichever
    // command names it.
    if let Err(error) = atlas_core::own_folder::refuse(&config) {
        report_failure(format, &error, !machine);
        return ExitCode::FAILURE;
    }

    // How long a command that waits for control may wait; other commands
    // never read `ATLAS_WAIT`.
    let wait = if policy.waits(dispatch::actor()) {
        match cli::control_wait_given(cli.wait, std::env::var_os("ATLAS_WAIT").as_deref()) {
            Ok(wait) => wait,
            Err(error) => return help::usage(error),
        }
    } else {
        std::time::Duration::ZERO
    };

    // A supervisor starts its work before anything else of Atlas starts
    // in it, and only for the machine guest's own daemon.
    if let Command::MachineSupervise { words } = &command {
        if let Err(error) = atlas_daemon::worker::machine_supervisor() {
            eprintln!("{}", json!(error));
            return ExitCode::from(atlas_daemon::machine::supervise::UNSTARTED);
        }
        return ExitCode::from(atlas_daemon::machine::supervise::start(words));
    }

    // A background job's first process becomes the job itself, before
    // anything else of Atlas starts in it.
    if let Command::JobLaunch {
        log,
        argv0,
        program,
    } = &command
    {
        return launch_job(log, argv0, program);
    }

    // A machine desktop's setup phase runs alone, as root, before anything else.
    #[cfg(target_os = "macos")]
    if let Command::MachineSetup(phase) = command {
        return machine_setup(phase);
    }

    // A VM desktop's agent and removal run alone, before anything else.
    if let Command::Vm(asked) = command {
        return vm(asked);
    }

    // What the station keeps for a VM is kept alone, before anything else.
    if let Command::Station(asked) = command {
        return station(asked, &config, cli.allow_disrupt);
    }

    // A machine desktop's root installer runs alone, before anything else.
    #[cfg(target_os = "macos")]
    if matches!(command, Command::MachineInstaller) {
        return match atlas_daemon::machine::installer::serve_blocking() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{}", json!(error));
                ExitCode::FAILURE
            }
        };
    }

    // A machine desktop's daemon keeps this thread for its run loop too.
    #[cfg(target_os = "macos")]
    if let Command::Daemon {
        desktop_id,
        enrolment,
        ..
    } = &command
    {
        return machine_daemon(desktop_id, enrolment);
    }

    // The helper daemon keeps this thread for the Core Foundation run loop.
    if let Command::HelperServe(service) = command {
        // The account it names is a distinct helper first, for the clearest
        // word, then: only the job launchd runs as the helper serves — a
        // hand-started copy, which could name another account as its
        // controller, never reaches any initialization or endpoint.
        let judged = atlas_core::session::validate_target_uid(service.uid, service.controller_uid)
            .and_then(|()| {
                atlas_daemon::user::worker::serving(service.uid, service.controller_uid)
            });
        if let Err(error) = judged {
            eprintln!("{}", json!(error));
            return ExitCode::FAILURE;
        }
        let served = atlas_daemon::daemon::serve::serve_blocking(
            service.uid,
            service.controller_uid,
            service.socket,
        );
        return match served {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{}", json!(error));
                ExitCode::FAILURE
            }
        };
    }

    choose_desktop(&config, cli.desktop.take(), &command);

    run(config, command, policy, wait, cli.allow_disrupt, format)
}

/// The session this command's desktop was chosen for, so a session the
/// helper's verification finds later is held to its own binding.
static CHOSEN_FOR: std::sync::OnceLock<Option<atlas_core::host::Key>> = std::sync::OnceLock::new();

/// Choose this command's desktop, once, before it does anything: the one
/// `flag` names, then the one its parent Atlas process chose, then
/// `ATLAS_DESKTOP`, then its agent session's, then the default. An agent
/// session is bound at its first command, whatever that command is. The process keeps the choice, or why there is none, for every
/// later settings load. `atlas stop` chooses nothing: it stops every
/// desktop. The Second Login's own setup, update and removal work on
/// it, whatever is chosen.
fn choose_desktop(config: &std::path::Path, flag: Option<String>, command: &Command) {
    use atlas_client::host_session::desktops::Bindings;
    use atlas_core::config::desktops::{self, Selection, Settings};

    let selection = Selection::from_env(flag);
    match command {
        // A Stop is for every desktop. Workers that run inside the helper
        // never read the controller's settings, its bindings or its locks.
        Command::Stop
        | Command::SetupHelper(_)
        | Command::InputService(_)
        | Command::EnvironmentWorker { .. }
        | Command::SessionInfo
        | Command::PermissionsInfo
        | Command::ElementsInfo { .. } => return,
        Command::Setup(_)
        | Command::Update(_)
        | Command::UpdateFinish
        | Command::Init
        | Command::Uninstall(_)
        | Command::Onboarding { .. } => {
            let user = Ok(atlas_core::desktop::RecordKey::user());
            return desktops::choose_for_process(selection, user);
        }
        _ => {}
    }
    // Settings that do not load are this command's refusal, kept so a file
    // that appears later in the process changes nothing; the commands that
    // set Atlas up are above. The lock is never taken for them, so a
    // helper-side command touches nothing of the helper's account.
    if let Err(error) = Settings::load(config) {
        return desktops::choose_for_process(selection, Err(error));
    }
    // Every command of an agent session is held to its binding, whether or
    // not its work is attributed to the session. A child of another Atlas
    // command acts on the desktop its parent chose and bound.
    let session = selection
        .parent
        .is_none()
        .then(atlas_core::host::HostSession::from_env)
        .flatten()
        .map(|session| session.key().clone());
    let chosen = Bindings::of_account()
        .and_then(|bindings| bindings.choose(config, &selection, session.as_ref()));
    if selection.parent.is_none() {
        let _ = CHOSEN_FOR.set(session);
    }
    // Never silent: with a choice of desktops, a session that took the
    // default is told which and how to choose another.
    if let Ok(chosen) = &chosen
        && chosen.told
    {
        let _ = writeln!(
            std::io::stderr().lock(),
            "This session uses the desktop \"{}\", the default. Name another with \
             --desktop or ATLAS_DESKTOP.",
            chosen.name
        );
    }
    desktops::choose_for_process(selection, chosen.map(|chosen| chosen.key));
}

/// Hold `verified`, the session the helper's verification settled on, to
/// its own binding when the choice was made for another, or for none.
///
/// # Errors
///
/// Returns the failure to choose for `verified`, or the refusal of a
/// different desktop.
fn confirm_desktop(
    config: &std::path::Path,
    verified: &atlas_core::host::HostSession,
) -> Result<(), Error> {
    use atlas_client::host_session::desktops::Bindings;

    let Some(used) = CHOSEN_FOR.get() else {
        return Ok(());
    };
    if used.as_ref() == Some(verified.key()) {
        return Ok(());
    }
    Bindings::of_account()?.confirm(config, verified.key())
}

/// Keep runtime, interrupt ownership and result reporting alive for one command.
fn run(
    config: std::path::PathBuf,
    command: Command,
    policy: Policy,
    wait: std::time::Duration,
    allow_disrupt: bool,
    format: Format,
) -> ExitCode {
    let machine = policy.machine_only();
    // A refusing environment worker exits distinctly, so its caller can tell
    // the refusal from the command it would have run.
    let worker = match &command {
        Command::EnvironmentWorker { packet, .. } => Some(
            packet
                .parent()
                .and_then(|request| request.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        _ => None,
    };
    // `atlas mcp` reads its host's requests until the host closes the pipe,
    // and a host that ends it with a signal may never close it.
    let reads_stdin = matches!(command, Command::Mcp(_));

    // Commands that ask questions or wait for the person stop cleanly on
    // Ctrl-C, saying what that means for each. Those that run agent tools
    // stop them whenever they are stopped, a person watching or not. Every
    // other command that takes control, whoever runs it, stops on a signal
    // by starting nothing more and letting the step under way settle.
    let (interrupted, runs_agents, listens) = (
        policy.interrupted(),
        policy.runs_agents(),
        policy.controls(),
    );
    let interrupt = Interrupt::process();

    // The runtime lasts until the process exits, so a signal that comes
    // while the outcome is being printed is still heard.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            report_failure(format, &Error::from(error), !machine);
            return ExitCode::FAILURE;
        }
    };
    let result = runtime
        .block_on(async {
            let message = interrupted.filter(|_| ui::interactive());
            if message.is_some() || runs_agents {
                stop_cleanly_on_interrupt(message);
            } else if listens {
                interrupt.listen(move |stopped| report_stop(format, stopped, !machine));
            }

            interrupt
                .unless_stopped(async {
                    let result =
                        dispatch::execute(config.clone(), command, &policy, wait, allow_disrupt)
                            .await;

                    // Being stopped: the stop ends Atlas in a moment and says
                    // so, so what the cut-short command gave is not reported.
                    if atlas_agents::stopping() {
                        std::future::pending::<()>().await;
                    }

                    result
                })
                .await
        })
        .map_err(|error| or_logged_out(error, &config));
    interrupt.ended();

    let status = match result {
        Ok(result) => {
            output::emit(format, &policy.view, &result, !machine);
            exit_status(output::failed(&result), interrupt.asked())
        }
        Err(error) => {
            if let Some(request) = worker {
                let mut refusal = json!(error);
                refusal["refusal"] = json!(request);
                eprintln!("{refusal}");
                return ExitCode::from(atlas_client::environment::WORKER_REFUSED);
            }

            report_failure(format, &error, !machine);
            exit_status(true, interrupt.asked())
        }
    };
    // The picture a moment after an agent's action is another process's
    // to take. The picture before it may still be being saved: it has a
    // moment more, never longer.
    if let Some(before) = policy
        .doing
        .as_ref()
        .and_then(atlas_client::doing::Doing::picture)
    {
        picture_after(&config, &before);
    }
    atlas_client::before::settle(std::time::Duration::from_millis(500));
    // Its own cleanup done, nothing it started still matters: the read of
    // standard input left waiting would otherwise keep the process from
    // ending until the host kills it.
    if reads_stdin {
        runtime.shutdown_background();
    }
    status
}

/// Start the process that takes the picture a moment after the action whose
/// picture before is `before`, on its own: no agent waits for it, and it
/// outlives this command. One that cannot start leaves the action with its
/// picture before alone.
fn picture_after(config: &std::path::Path, before: &str) {
    use std::os::unix::process::CommandExt;

    let Ok(atlas) = std::env::current_exe() else {
        return;
    };
    let ended = atlas_core::time::now_ms().to_string();
    let mut command = std::process::Command::new(atlas);
    command
        .arg("--config")
        .arg(config)
        .args(["picture-after", "--before", before, "--ended", &ended])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    // It pictures this command's desktop.
    if let Some((variable, key)) = atlas_core::config::desktops::for_children() {
        command.env(variable, key);
    }
    // Not waited for: it ends on its own, a moment later.
    let _ = atlas_core::spawn::spawn(&mut command);
}

/// Print a failure on stderr as `format` asks, with what to do about it
/// when `advised`. A closed stderr loses it, and nothing else.
fn report_failure(format: Format, error: &Error, advised: bool) {
    let text = match format {
        Format::Json => output::failure(error, advised).to_string(),
        Format::Human => render::error(error),
    };
    let _ = writeln!(std::io::stderr().lock(), "{text}");
}

/// Refuse unless the helper daemon started this and made its standard
/// input, then wait for its `go` there, become a session and group
/// leader of its own, then replace itself with the job's program, its
/// output going to `log` and its input nowhere.
/// Says `setsid` on the status pipe (standard output) first; a failure
/// after that is written there too, and the pipe closes on a successful
/// exec because only a close-on-exec copy of it is kept. A group leader
/// cannot call setsid, which is why the daemon starts this process rather
/// than the program itself in a group of its own.
fn launch_job(log: &std::path::Path, argv0: &str, program: &[String]) -> ExitCode {
    use std::{
        fs::OpenOptions,
        os::{
            fd::AsFd,
            unix::{fs::OpenOptionsExt, process::CommandExt},
        },
        process::Stdio,
    };

    let Ok(status) = std::io::stdout().as_fd().try_clone_to_owned() else {
        return ExitCode::from(127);
    };
    let mut status = std::fs::File::from(status);
    let mut say = |line: &str| {
        let _ = writeln!(status, "{line}");
    };
    // Only the helper daemon starts a job, and only its socket says go.
    if let Err(error) = atlas_daemon::user::worker::job_launcher() {
        say(&format!("error {}", error.error));
        return ExitCode::from(127);
    }
    // The daemon says to go on once its record of this job is written; a
    // daemon that ended before then leaves nothing to run.
    let mut go = String::new();
    if std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut go).is_err()
        || go.trim_end() != "go"
    {
        return ExitCode::from(127);
    }
    if let Err(error) = nix::unistd::setsid() {
        say(&format!("error a session of its own: {error}"));
        return ExitCode::from(127);
    }
    say("setsid");

    let Some((executable, arguments)) = program.split_first() else {
        say("error no program to run");
        return ExitCode::from(127);
    };
    let opened = OpenOptions::new()
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(log);
    let (output, errors) = match opened.and_then(|log| Ok((log.try_clone()?, log))) {
        Ok(files) => files,
        Err(error) => {
            say(&format!("error the job's log: {error}"));
            return ExitCode::from(127);
        }
    };

    let error = std::process::Command::new(executable)
        .arg0(argv0)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(errors)
        .exec();
    say(&format!("error {error}"));
    ExitCode::from(127)
}

/// The status a command ends with: the signal's once one stopped it, else
/// its own.
fn exit_status(failed: bool, stopped: Option<Signal>) -> ExitCode {
    match stopped {
        Some(signal) => ExitCode::from(atlas_control::interrupt::status(signal)),
        None if failed => ExitCode::FAILURE,
        None => ExitCode::SUCCESS,
    }
}

/// Say how a command a signal stopped ended, when it never got as far as
/// saying so itself. A person's terminal first loses a spinner's line and
/// gets its cursor back.
fn report_stop(format: Format, stopped: &Stopped, advised: bool) {
    if format == Format::Human && std::io::stderr().is_terminal() {
        let terminal = console::Term::stderr();
        let _ = terminal.clear_line();
        let _ = terminal.show_cursor();
    }
    report_failure(format, &stopped.error(), advised);
}

/// A helper that did not answer because it is logged out says so, in place
/// of "not answering": an agent cannot fix that, but the person can.
fn or_logged_out(error: Error, config: &std::path::Path) -> Error {
    if error.code != Code::Transport || error.uncertain {
        return error;
    }

    let Ok(config) = Config::load(config) else {
        return error;
    };

    // This Mac can see whether the second login is logged out; a VM
    // desktop's settings do not load as a helper's.
    match atlas_core::user::logins::absence(config.user().uid) {
        Some(atlas_core::session::Absence::LoggedOut(_)) => Error::new(
            Code::LoggedOut,
            "The helper is logged out (after a restart or a log-out)",
        ),
        _ => error,
    }
}

/// Whether launchd started this as the app (opened from Finder and the
/// like) rather than a shell running `atlas`.
fn opened_as_app() -> bool {
    std::os::unix::process::parent_id() == 1 && !std::io::stdin().is_terminal()
}

/// The command a set-up Atlas runs when opened as an app: `atlas monitor`
/// starts the menu bar when it is not running and then shows the viewer
/// window. Starting only the menu bar leaves a second click with nothing to
/// show when it already runs, which reads as the app not opening.
fn viewer_command(executable: &std::path::Path, config: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new(executable);
    command.arg("--config").arg(config).arg("monitor");
    command
}

/// Opened as an app: the onboarding window until Atlas is set up here (or
/// in the helper another account chose), the viewer window after that. Either
/// replaces this process, so the app stays the one the person opened.
fn open_app(config: &std::path::Path) -> ExitCode {
    use std::os::unix::process::CommandExt;

    let Ok(executable) = std::env::current_exe() else {
        return ExitCode::FAILURE;
    };

    let finished = !crate::user::open::chosen_as_helper()
        // A VM desktop's settings do not load as a helper's: not set up here.
        && Config::load(config).is_ok_and(|config| crate::user::open::set_up_in(&config.user()));
    let error = if finished {
        viewer_command(&executable, config).exec()
    } else {
        match atlas_cli::menubar::app_program() {
            Ok(window) => std::process::Command::new(window)
                .arg("--onboarding")
                .env("ATLAS_CLI", &executable)
                .env("ATLAS_CONFIG", config)
                .exec(),
            Err(error) => {
                eprintln!("{}", error.error);
                return ExitCode::FAILURE;
            }
        }
    };

    eprintln!("Atlas could not open: {error}");
    ExitCode::FAILURE
}

/// Ctrl-C during a question or a wait puts the cursor back and says how to
/// resume (`message`, when a person is asked), instead of leaving a
/// half-drawn line. A question Ctrl-C ends (`prompt`) waits a second
/// for this to print and exit, so the person sees one message; change the
/// two together. Being told to stop (SIGTERM) or losing the terminal
/// (SIGHUP) stops the same way, without a message.
fn stop_cleanly_on_interrupt(message: Option<&'static str>) {
    use tokio::signal::unix::{SignalKind, signal};

    let (Ok(mut terminate), Ok(mut hangup)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::hangup()),
    ) else {
        return;
    };

    tokio::spawn(async move {
        let (code, said) = tokio::select! {
            Ok(()) = tokio::signal::ctrl_c() => (130, message),
            Some(()) = terminate.recv() => (143, None),
            Some(()) = hangup.recv() => (129, None),
            else => return,
        };

        // An agent tool Atlas started in its own process group does not see
        // the terminal's Ctrl-C: stop it too, before saying stopped. No tool
        // starts after this, and the command stops at its next step. Stopping
        // waits for the tools to be reaped, so it runs off the runtime; a
        // failed join still exits, as the process is ending either way.
        let _ = tokio::task::spawn_blocking(atlas_agents::stop_running).await;

        if let Some(message) = said {
            let _ = console::Term::stderr().show_cursor();
            let _ = writeln!(std::io::stderr().lock(), "\n{} {message}", style::unknown());
        }

        std::process::exit(code);
    });
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

/// Serve as a machine desktop's daemon, for the desktop `desktop_id` and
/// the enrolment `enrolment` its installed job names.
#[cfg(target_os = "macos")]
fn machine_daemon(desktop_id: &str, enrolment: &str) -> ExitCode {
    use atlas_core::{desktop::DesktopId, files::Token};
    use atlas_daemon::daemon::serve::machine::entry::{Named, serve_blocking};
    let (Some(desktop), Some(enrolment)) = (DesktopId::parse(desktop_id), Token::parse(enrolment))
    else {
        eprintln!(
            "{}",
            json!(atlas_core::error::Error::new(
                atlas_core::error::Code::BadRequest,
                "The machine job names no valid desktop or enrolment."
            ))
        );
        return ExitCode::FAILURE;
    };
    match serve_blocking(Named { desktop, enrolment }) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", json!(error));
            ExitCode::FAILURE
        }
    }
}

/// Do what the station asks of a VM desktop, and print what it came to as
/// one JSON line: only when an Atlas command asked.
fn vm(asked: cli::Vm) -> ExitCode {
    use atlas_core::{desktop::DesktopId, files::Token};
    let invalid = || {
        atlas_core::error::Error::new(
            atlas_core::error::Code::BadRequest,
            "That names no valid desktop or change.",
        )
    };
    let done = atlas_daemon::worker::started_by_atlas().and_then(|()| match asked {
        cli::Vm::InstallAgent { desktop_id } => DesktopId::parse(&desktop_id)
            .ok_or_else(invalid)
            .and_then(|desktop| atlas_client::vm::install_agent(&desktop)),
        cli::Vm::Remove { desktop_id, change } => DesktopId::parse(&desktop_id)
            .zip(Token::parse(&change))
            .ok_or_else(invalid)
            .and_then(|(desktop, change)| atlas_client::vm::remove(&desktop, &change)),
    });
    match done {
        Ok(found) => {
            println!("{found}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", json!(error));
            ExitCode::FAILURE
        }
    }
}

/// What the station keeps for a VM desktop, and print what it came to as one
/// JSON line: an enrolment, its secret read from standard input, and the
/// keeper's agent only when an Atlas command asked; the keeper itself only
/// as its own launchd job.
fn station(asked: cli::Station, settings: &std::path::Path, allow_disrupt: bool) -> ExitCode {
    use atlas_client::{
        station::enrolment,
        vm::{handover, keeper},
    };
    let done = match asked {
        cli::Station::Enrol { key, enrolment } => atlas_daemon::worker::started_by_atlas()
            .and_then(|()| {
                enrolment::enrol(
                    &enrolment::Keychain,
                    &key,
                    &enrolment,
                    &mut std::io::stdin().lock(),
                )
            }),
        cli::Station::InstallKeeper { key } => atlas_daemon::worker::started_by_atlas()
            .and_then(|()| enrolment::vm_of(&key))
            .and_then(|vm| keeper::install_keeper(&vm, settings)),
        cli::Station::Handover {
            key,
            settings: pinned,
        } => atlas_daemon::worker::started_by_atlas()
            .and_then(|()| handover::hand_over(&key, &pinned, allow_disrupt)),
        cli::Station::Keep { key, settings } => enrolment::vm_of(&key)
            .and_then(|vm| atlas_daemon::worker::launchd_job(&keeper::label(&vm)))
            .and_then(|()| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .map_err(|error| {
                        atlas_core::error::Error::new(
                            atlas_core::error::Code::Startup,
                            "The keeper could not start its runtime.",
                        )
                        .with_source(error)
                    })
            })
            .and_then(|runtime| runtime.block_on(keeper::keep(&key, &settings)))
            .map(|()| json!({"ok": true})),
    };
    match done {
        Ok(found) => {
            println!("{found}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", json!(error));
            ExitCode::FAILURE
        }
    }
}

/// Run one phase of putting Atlas inside a machine desktop's guest, and
/// print what it found as one JSON line.
#[cfg(target_os = "macos")]
fn machine_setup(phase: cli::MachineSetup) -> ExitCode {
    use atlas_core::{desktop::DesktopId, files::Token};
    use atlas_daemon::machine::setup::{Step, run};
    let invalid = || {
        atlas_core::error::Error::new(
            atlas_core::error::Code::BadRequest,
            "A setup phase names no valid desktop, enrolment or change.",
        )
    };
    let step = match phase {
        cli::MachineSetup::Publish { change, source } => {
            Token::parse(&change).map(|change| Step::Publish { change, source })
        }
        cli::MachineSetup::Jobs {
            desktop_id,
            enrolment,
            account,
            paired,
        } => DesktopId::parse(&desktop_id)
            .zip(Token::parse(&enrolment))
            .map(|(desktop, enrolment)| Step::Jobs {
                desktop,
                enrolment,
                account,
                paired,
            }),
        cli::MachineSetup::Admin { name } => Some(Step::Admin { name }),
        cli::MachineSetup::Demote { account } => Some(Step::Demote { account }),
        cli::MachineSetup::Close => Some(Step::Close),
    };
    let done = step
        .ok_or_else(invalid)
        .and_then(|step| run(step, &mut std::io::stdin()));
    match done {
        Ok(found) => {
            println!("{found}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", json!(error));
            ExitCode::FAILURE
        }
    }
}
