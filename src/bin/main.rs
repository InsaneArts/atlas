mod cli;
mod dispatch;
mod doctor;
mod help;
mod output;
mod policy;
mod update;

use atlas::{
    client::Client,
    config::Config,
    error::Error,
    interrupt::{Interrupt, Stopped},
    menubar,
    ui::{self, Format, render, style},
};
use clap::FromArgMatches;
use cli::{Cli, Command};
use nix::sys::signal::Signal;
use policy::Policy;
use serde_json::{Value, json};
use std::{
    io::{IsTerminal, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    let matches = match help::command().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => return help::usage(error),
    };

    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    let config = atlas::config::effective(cli.config.take());

    // `atlas` alone shows what it can do; opened as an app (Finder,
    // Launchpad, `open`), it shows the window instead. launchd names the
    // job that started it: the helper's one-time opener opens the window
    // once the desktop is up.
    let Some(command) = cli.command.take() else {
        if opened_as_app() {
            if atlas::setup::first_login::started_as_opener() {
                // Its output goes nowhere; the next login tries again.
                return match atlas::setup::first_login::open_at_login() {
                    Ok(_) => ExitCode::SUCCESS,
                    Err(_) => ExitCode::FAILURE,
                };
            }
            return open_app(&config);
        }
        let _ = help::command().print_help();
        return ExitCode::SUCCESS;
    };

    let policy = Policy::of(&command);

    // Commands run by Atlas itself always answer in JSON, and their
    // failures carry no step for an agent: another Atlas process reads them.
    let machine = policy.machine_only;
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

    // The helper daemon keeps this thread for the Core Foundation run loop.
    if let Command::HelperServe {
        uid,
        controller_uid,
        socket,
    } = command
    {
        return match atlas::daemon::serve::serve_blocking(uid, controller_uid, socket) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{}", json!(error));
                ExitCode::FAILURE
            }
        };
    }

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

    // Commands that ask questions or wait for the person stop cleanly on
    // Ctrl-C, saying what that means for each. Those that run agent tools
    // stop them whenever they are stopped, a person watching or not. Every
    // other command that takes control, whoever runs it, stops on a signal
    // by starting nothing more and letting the step under way settle.
    let (interrupted, runs_agents, listens) =
        (policy.interrupted, policy.runs_agents, policy.controls);
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
                    let result = dispatch::execute(config.clone(), command, &policy, wait).await;

                    // Being stopped: the stop ends Atlas in a moment and says
                    // so, so what the cut-short command gave is not reported.
                    if atlas::agents::stopping() {
                        std::future::pending::<()>().await;
                    }

                    result
                })
                .await
        })
        .map_err(|error| or_logged_out(error, &config));
    interrupt.ended();

    match result {
        Ok(result) => {
            output::emit(format, &policy.view, &result, !machine);
            exit_status(output::failed(&result), interrupt.asked())
        }
        Err(error) => {
            if let Some(request) = worker {
                let mut refusal = json!(error);
                refusal["refusal"] = json!(request);
                eprintln!("{refusal}");
                return ExitCode::from(atlas::environment::WORKER_REFUSED);
            }

            report_failure(format, &error, !machine);
            exit_status(true, interrupt.asked())
        }
    }
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

/// Wait for the daemon's `go` on standard input, become a session and group
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
        Some(signal) => ExitCode::from(atlas::interrupt::status(signal)),
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
    if error.code != "transport" || error.uncertain {
        return error;
    }

    let Ok(config) = Config::load(config) else {
        return error;
    };

    match atlas::session::absence(config.helper_uid) {
        Some(atlas::session::Absence::LoggedOut(_)) => Error::new(
            "logged-out",
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

/// Opened as an app: the onboarding window until Atlas is set up here (or
/// in the helper another account chose), the menu bar after that. Either
/// replaces this process, so the app stays the one the person opened.
fn open_app(config: &std::path::Path) -> ExitCode {
    use std::os::unix::process::CommandExt;

    let Ok(executable) = std::env::current_exe() else {
        return ExitCode::FAILURE;
    };

    let me = nix::unistd::getuid().as_raw();
    let helper_here = atlas::onboarding::request().is_some_and(|pairing| pairing.helper_uid == me);
    let finished = !helper_here
        && Config::load(config).is_ok_and(|config| {
            atlas::setup::migrate::settled(config.helper_uid)
                && atlas::setup::account_with_uid(config.helper_uid)
                    .is_ok_and(|account| account.is_some())
        });
    let error = if finished {
        std::process::Command::new(&executable)
            .arg("--config")
            .arg(config)
            .arg("menubar")
            .exec()
    } else {
        match menubar::executable() {
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
/// half-drawn line. A question Ctrl-C ends (`ui::prompt`) waits a second
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
        // starts after this, and the command stops at its next step.
        atlas::agents::stop_running();

        if let Some(message) = said {
            let _ = console::Term::stderr().show_cursor();
            let _ = writeln!(std::io::stderr().lock(), "\n{} {message}", style::unknown());
        }

        std::process::exit(code);
    });
}

/// `atlas doctor`: the helper's readiness, with the checks only this side
/// can make (the account itself, and a leftover session of a deleted one).
async fn doctor(client: &Client, verbose: bool) -> Value {
    let report = doctor::collect(client).await;

    // People see the summary either way; programs get the full status
    // only when they ask for it.
    if verbose || ui::human() {
        report
    } else {
        atlas::capability::summary(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signal_gives_its_status_whatever_the_result() {
        assert_eq!(exit_status(false, None), ExitCode::SUCCESS);
        assert_eq!(exit_status(true, None), ExitCode::FAILURE);
        for failed in [false, true] {
            assert_eq!(
                exit_status(failed, Some(Signal::SIGTERM)),
                ExitCode::from(143)
            );
            assert_eq!(
                exit_status(failed, Some(Signal::SIGHUP)),
                ExitCode::from(129)
            );
            assert_eq!(
                exit_status(failed, Some(Signal::SIGINT)),
                ExitCode::from(130)
            );
        }
    }
}
