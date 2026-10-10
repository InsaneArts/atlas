mod help;

use clap::{FromArgMatches, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{
    io::{IsTerminal, Read, Write},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};
use understudy::{
    actions::Button,
    agent,
    client::Client,
    config::Config,
    environment,
    error::{Error, Result},
    menubar,
    transport::with_output,
    ui::{
        self, Format,
        progress::Spinner,
        render::{self, View},
        style, text,
    },
    viewer,
};

#[derive(Parser)]
#[command(
    version,
    about = "A hidden second macOS desktop where agents test apps while you keep working"
)]
struct Cli {
    /// Print JSON (the default when the output is not a terminal).
    #[arg(long, global = true, help_heading = "Global options")]
    json: bool,
    /// Use another settings file.
    #[arg(
        long,
        value_name = "FILE",
        env = "UNDERSTUDY_CONFIG",
        hide_env = true,
        hide_default_value = true,
        help_heading = "Global options",
        default_value = understudy::config::DEFAULT_PATH,
        global = true
    )]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Copy selected settings and files into the helper.
    #[command(
        args_conflicts_with_subcommands = true,
        long_about = "Copy selected settings and files into the helper.\n\nPreview an explicit one-way snapshot into an existing hidden login. The TOML manifest lists [[files]] tables: source (absolute or relative to the manifest), target (ASCII relative path inside the helper home), and optional executable. Stop applications that write these selected settings before applying. Existing untracked differences and helper-side edits are conflicts; managed updates keep backups. This does not create accounts, copy logins, or synchronize running application databases."
    )]
    Environment {
        /// TOML manifest listing only the files to provision.
        manifest: Option<PathBuf>,
        /// Apply the previewed changes; otherwise inspect without changing selected files.
        #[arg(long)]
        apply: bool,
        #[command(subcommand)]
        command: Option<EnvironmentCommand>,
    },
    #[command(hide = true)]
    EnvironmentWorker {
        packet: PathBuf,
        #[arg(long)]
        apply: bool,
    },
    #[command(hide = true)]
    SessionInfo,
    #[command(hide = true)]
    WindowsInfo,
    #[command(hide = true)]
    ElementsInfo { pid: i32 },
    /// The helper daemon inside the helper login (started by its LaunchAgent).
    #[command(hide = true)]
    HelperServe {
        #[arg(long)]
        uid: u32,
        #[arg(long)]
        controller_uid: u32,
        #[arg(long)]
        socket: PathBuf,
    },
    #[command(hide = true)]
    InputService {
        #[arg(long)]
        uid: u32,
        #[arg(long)]
        controller_uid: u32,
        #[arg(long)]
        socket: PathBuf,
    },
    /// Inspect or repair the helper's setup (advanced).
    #[command(
        long_about = "Inspect or repair the helper's setup (advanced).\n\nWithout flags, report the helper account, configuration, endpoints and services without changing anything. --apply runs Understudy's own signed helper daemon as a managed LaunchAgent on a private socket directory beside the existing capture job, moves input and the configuration over once it is verified, then retires the old job, recording every step so an interruption resumes. --rollback restores the previous job and configuration."
    )]
    Setup {
        /// Helper account short name. Defaults to the configured helper.
        #[arg(long, conflicts_with_all = ["apply", "rollback"])]
        helper: Option<String>,
        /// Migrate (or resume migrating) to the managed private layout.
        #[arg(long, conflicts_with = "rollback")]
        apply: bool,
        /// Undo an applied or interrupted migration.
        #[arg(long)]
        rollback: bool,
    },
    #[command(hide = true)]
    SetupHelper { request: String },
    /// Show where Understudy keeps its settings, sockets and screenshots.
    Config,
    /// Show the helper: who it is, where it is and what it may do.
    Status,
    /// Check that everything is ready, and say what to fix if not.
    Doctor {
        /// Include the helper's full status (in --json too).
        #[arg(long)]
        verbose: bool,
    },
    /// Check that native Save and Open dialogs work in the helper.
    ///
    /// Exercises real Save/Open dialogs with the AppKit fixture and records the outcome.
    VerifyNative {
        /// Compiled tests/live/DesktopFixture.swift inside a helper-readable path.
        #[arg(long, default_value = understudy::capability::FIXTURE)]
        fixture: PathBuf,
    },
    /// Put this build where the helper can run it.
    Install,
    /// Start (or reset) the helper's input service.
    StartInput,
    /// Watch and control the helper in your browser.
    Monitor {
        /// Local port for the viewer.
        #[arg(long, default_value_t = 8765)]
        port: u16,
    },
    /// Show the helper in the menu bar, to watch it or take control.
    ///
    /// The menu bar runs on its own, so the terminal can close. Quit it from
    /// its menu; that stops its viewer and releases control.
    Menubar {
        /// Local port for the viewer it opens.
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// Serve the menu bar from this process (how it runs on its own).
        #[arg(long, hide = true, conflicts_with_all = ["render", "login"])]
        serve: bool,
        /// Instead, draw the panel in light and dark to PNGs in DIR and exit.
        #[arg(long, value_name = "DIR")]
        render: Option<PathBuf>,
        /// Instead, turn "Open at Login" on or off, or report it.
        #[arg(long, value_name = "on|off|status", conflicts_with = "render")]
        login: Option<LoginArg>,
    },
    /// Save a screenshot of the helper's screen.
    ///
    /// Reports pointsPerPixel: multiply screenshot pixels by it for input coordinates.
    #[command(visible_alias = "capture")]
    Screenshot {
        /// Scale the image down so its longer side is at most this many pixels.
        #[arg(long)]
        max_dimension: Option<u32>,
    },
    /// List the apps open in the helper.
    Apps,
    /// Send several actions at once (a JSON array, or @path/to/actions.json).
    Input {
        /// A JSON array of actions, or @path/to/actions.json.
        actions: String,
    },
    /// Click a point, or exactly one element of an app.
    ///
    /// Click at display point X Y (screenshot pixels × pointsPerPixel), or on
    /// exactly one accessibility element of an app with --app and --name/--role.
    Click {
        /// Display point X (screenshot pixels × pointsPerPixel).
        #[arg(requires = "y")]
        x: Option<f64>,
        /// Display point Y.
        y: Option<f64>,
        #[command(flatten)]
        target: AppTarget,
        /// Exact element title, description or identifier.
        #[arg(long, conflicts_with = "x")]
        name: Option<String>,
        /// Accessibility role, for example AXButton.
        #[arg(long, conflicts_with = "x")]
        role: Option<String>,
        /// Which mouse button.
        #[arg(long, value_enum, default_value_t)]
        button: ButtonArg,
        /// Clicks in a row: 2 for a double click.
        #[arg(long, default_value_t = 1)]
        count: u8,
        /// Comma-separated: cmd, ctrl, alt, shift, fn.
        #[arg(long, value_delimiter = ',')]
        modifiers: Vec<String>,
    },
    /// List an app's buttons, fields and other elements.
    ///
    /// Lists accessibility elements of the app's windows: role, name, value and frame.
    Elements {
        #[command(flatten)]
        target: AppTarget,
        /// Only elements with this accessibility role, for example AXButton.
        #[arg(long)]
        role: Option<String>,
        /// Only elements with this exact title, description or identifier.
        #[arg(long)]
        name: Option<String>,
        /// Show at most this many elements.
        #[arg(long, default_value_t = 200)]
        max: usize,
    },
    /// Print the guide agents read to use Understudy.
    AgentGuide {
        /// Print the agent plugin's skill (SKILL.md) instead.
        #[arg(long, hide = true, conflicts_with = "commands")]
        skill: bool,
        /// Print the command reference (Markdown) instead.
        #[arg(long, hide = true)]
        commands: bool,
    },
    /// Check that tools and apps are ready in the helper.
    Readiness {
        /// A command-line tool to check (repeat for more).
        #[arg(long = "tool", value_name = "NAME")]
        tools: Vec<String>,
        /// An app to check (repeat for more).
        #[arg(long = "app", value_name = "NAME")]
        apps: Vec<String>,
    },
    /// Move the pointer to a point.
    Move {
        /// Display point X.
        x: f64,
        /// Display point Y.
        y: f64,
    },
    /// Drag from one point to another.
    Drag {
        /// Start point X.
        from_x: f64,
        /// Start point Y.
        from_y: f64,
        /// End point X.
        to_x: f64,
        /// End point Y.
        to_y: f64,
        /// Which mouse button.
        #[arg(long, value_enum, default_value_t)]
        button: ButtonArg,
    },
    /// Scroll at a point.
    ///
    /// Scroll at X Y by lines; negative values scroll up or left.
    Scroll {
        /// Display point X.
        x: f64,
        /// Display point Y.
        y: f64,
        /// Lines to scroll sideways; negative scrolls left.
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        dx: i32,
        /// Lines to scroll; negative scrolls up.
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        dy: i32,
    },
    /// Type text.
    ///
    /// Long text is split into batches without breaking characters.
    Type {
        /// The text to type.
        #[arg(required_unless_present = "stdin")]
        text: Option<String>,
        /// Read the text from standard input instead.
        #[arg(long, conflicts_with = "text")]
        stdin: bool,
    },
    /// Press keys, for example cmd+s or escape.
    ///
    /// Keys are pressed in order: `key cmd+s` or `key escape return`.
    Key {
        /// Keys or chords such as cmd+s, escape, return, shift+tab.
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// List the helper's windows, front to back.
    ///
    /// Document windows only, unless --all.
    Windows {
        /// Only windows of this app.
        #[arg(long)]
        app: Option<String>,
        /// Only windows whose title contains this.
        #[arg(long)]
        title: Option<String>,
        /// Include panels, menus and other non-document windows.
        #[arg(long)]
        all: bool,
    },
    /// Open an app, files or URLs in the helper.
    Open {
        /// The app to open (or to open the targets with).
        #[arg(long)]
        app: Option<String>,
        /// A variable for the app's environment when it launches (repeat for more).
        #[arg(long = "env", value_name = "NAME=VALUE")]
        environment: Vec<String>,
        /// Files or URLs to open.
        #[arg(value_name = "FILE|URL")]
        targets: Vec<String>,
    },
    /// Quit an app in the helper and remove any copy `open` made of it.
    Close {
        /// The app's name, as `understudy apps` lists it.
        #[arg(long)]
        app: String,
    },
    /// Wait for a window to appear (or, with --gone, to close).
    Wait {
        /// A window of this app.
        #[arg(long)]
        app: Option<String>,
        /// A window whose title contains this.
        #[arg(long)]
        title: Option<String>,
        /// Wait for the window to close instead.
        #[arg(long)]
        gone: bool,
        /// Seconds.
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },

    /// Run a command in the helper and print its output.
    Run {
        /// The program and its arguments, after `--`.
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    /// Restart the helper after something went wrong.
    ///
    /// Confirms a new hidden helper before clearing an action of unknown outcome.
    Restart,
    /// Lock the screen so you can switch to the helper.
    ///
    /// You sign in on the lock screen; Understudy never sees a password.
    Switch {
        /// The account you will choose on the lock screen (the helper by default).
        account: Option<String>,
        /// Just lock the screen.
        #[arg(long, conflicts_with = "account")]
        lock: bool,
        /// List the accounts you can switch to.
        #[arg(long)]
        list: bool,
        /// Lock without asking first.
        #[arg(long)]
        yes: bool,
    },
    /// Update Understudy, checking its signature first.
    ///
    /// Verifies the release's checksum and Developer ID signature, then moves the helper onto it.
    Update {
        /// Accept a signed release that is not notarized.
        #[arg(long)]
        allow_unnotarized: bool,
        /// Reinstall even when the release is not newer.
        #[arg(long)]
        force: bool,
        /// Install this release tag instead of the latest.
        #[arg(long)]
        version: Option<String>,
    },
    /// Set Understudy up, step by step (safe to run again).
    ///
    /// Helper account, first sign-in, permissions, the helper job and input.
    Init,
    /// Remove Understudy from this Mac (asks first).
    ///
    /// The helper account and its files stay; the app goes to the Trash.
    Uninstall {
        /// Keep configuration, setup records and logs.
        #[arg(long)]
        keep_config: bool,
        /// Do not ask.
        #[arg(long)]
        yes: bool,
    },
    /// Connect Claude Code and Codex, or show which are connected.
    ///
    /// Adds a plugin that teaches the agent to use Understudy and guides setup.
    Agents {
        #[command(subcommand)]
        action: Option<AgentsCommand>,
    },
    /// After an update, run by the new executable: move the helper onto it.
    #[command(hide = true)]
    UpdateFinish,
}

#[derive(Subcommand)]
enum AgentsCommand {
    /// Which agents are on this Mac and whether Understudy is connected.
    Status {
        /// One plain line for an agent's session start.
        #[arg(long)]
        brief: bool,
    },
    /// Connect the plugin to Claude Code and/or Codex (all installed by default).
    Install {
        /// Only Claude Code.
        #[arg(long)]
        claude: bool,
        /// Only Codex.
        #[arg(long)]
        codex: bool,
    },
    /// Disconnect the plugin from Claude Code and/or Codex.
    Remove {
        /// Only Claude Code.
        #[arg(long)]
        claude: bool,
        /// Only Codex.
        #[arg(long)]
        codex: bool,
    },
}

/// The agents named by --claude/--codex (none means every installed one).
fn chosen(claude: bool, codex: bool) -> Vec<understudy::agents::Agent> {
    use understudy::agents::Agent;
    [(claude, Agent::Claude), (codex, Agent::Codex)]
        .into_iter()
        .filter_map(|(on, agent)| on.then_some(agent))
        .collect()
}

#[derive(Subcommand)]
enum EnvironmentCommand {
    /// Freeze selected files, exported variables and readiness requirements.
    Profile {
        /// TOML profile (version = 2) of files, tools, applications, environment and references.
        profile: PathBuf,
        /// Apply it; otherwise only preview.
        #[arg(long)]
        apply: bool,
    },
    /// Run a command with the selected helper environment and helper tool paths.
    Run {
        /// The program and its arguments, after `--`.
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    /// Apply or resume an immutable environment plan returned by preview.
    Apply { plan_id: String },
    /// Preview rollback, or restore matching managed state with --apply.
    Rollback {
        transaction_id: String,
        #[arg(long)]
        apply: bool,
    },
    /// Inspect selected tools, apps, variables and reference paths without launching them.
    Discover {
        #[arg(long = "tool")]
        tools: Vec<String>,
        #[arg(long = "app")]
        apps: Vec<String>,
        /// Report only whether the named variable is defined, never its value.
        #[arg(long = "env")]
        environment: Vec<String>,
        /// Check existing access without changing permissions.
        #[arg(long = "reference")]
        references: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    let matches = match help::command().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => return help::usage(error),
    };
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    cli.config = understudy::config::effective(cli.config);
    // `understudy` alone shows what it can do.
    let Some(command) = cli.command.take() else {
        let _ = help::command().print_help();
        return ExitCode::SUCCESS;
    };
    // Commands run by Understudy itself always answer in JSON.
    let format = if machine_only(&command) {
        Format::Json
    } else {
        ui::detect(
            cli.json,
            std::env::var("UNDERSTUDY_JSON").ok().as_deref(),
            std::io::stdout().is_terminal(),
        )
    };
    ui::set(format);
    // The helper daemon keeps this thread for the Core Foundation run loop.
    #[cfg(target_os = "macos")]
    if let Command::HelperServe {
        uid,
        controller_uid,
        socket,
    } = command
    {
        return match understudy::daemon::serve::serve_blocking(uid, controller_uid, socket) {
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
    let view = view(&command);
    // Commands that ask questions or wait for the person stop cleanly on
    // Ctrl-C, saying what that means for each.
    let interrupted = match command {
        Command::Init => Some("Stopped. Run understudy init again to continue."),
        Command::Switch { .. } | Command::Uninstall { .. } => Some("Stopped."),
        Command::Update { .. } => Some("Stopped. Run understudy update again to finish."),
        _ => None,
    };
    let result = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(Error::from)
        .and_then(|runtime| {
            runtime.block_on(async {
                if let Some(message) = interrupted.filter(|_| ui::interactive()) {
                    stop_cleanly_on_interrupt(message);
                }
                execute(cli.config, command).await
            })
        });
    match result {
        Ok(result) => {
            emit(format, &view, &result);
            if result["ok"] == false
                || result["ready"] == false
                || result["timedOut"] == true
                || result.get("exitCode").is_some_and(|v| v != 0)
            {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            if let Some(request) = worker {
                let mut refusal = json!(error);
                refusal["refusal"] = json!(request);
                eprintln!("{refusal}");
                return ExitCode::from(understudy::environment::WORKER_REFUSED);
            }
            match format {
                Format::Json => eprintln!("{}", json!(error)),
                Format::Human => eprintln!("{}", render::error(&error)),
            }
            ExitCode::FAILURE
        }
    }
}

/// Commands Understudy runs itself (in the helper, or as a step of another
/// command), whose output another Understudy process reads.
fn machine_only(command: &Command) -> bool {
    matches!(
        command,
        Command::EnvironmentWorker { .. }
            | Command::SessionInfo
            | Command::WindowsInfo
            | Command::ElementsInfo { .. }
            | Command::HelperServe { .. }
            | Command::InputService { .. }
            | Command::SetupHelper { .. }
            | Command::UpdateFinish
    )
}

/// How a command's result reads for a person.
fn view(command: &Command) -> View {
    let at = |x: f64, y: f64| format!("{}, {}", text::number(x), text::number(y));
    match command {
        Command::Doctor { verbose } => View::Doctor { verbose: *verbose },
        Command::Status => View::Status,
        Command::Config => View::Config,
        Command::Screenshot { .. } => View::Capture,
        Command::Apps => View::Apps,
        Command::Windows { .. } => View::Windows,
        Command::Elements { .. } => View::Elements,
        Command::Readiness { .. } => View::Readiness,
        Command::Click {
            x: Some(x),
            y: Some(y),
            ..
        } => View::Action(format!("Clicked at {}", at(*x, *y))),
        Command::Click { .. } => View::Action("Clicked".into()),
        Command::Move { x, y } => View::Action(format!("Moved the pointer to {}", at(*x, *y))),
        Command::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            ..
        } => View::Action(format!(
            "Dragged from {} to {}",
            at(*from_x, *from_y),
            at(*to_x, *to_y)
        )),
        Command::Scroll { x, y, .. } => View::Action(format!("Scrolled at {}", at(*x, *y))),
        Command::Key { keys } => View::Action(format!("Pressed {}", keys.join(" "))),
        Command::Input { .. } => View::Action("Performed the actions".into()),
        Command::Type { .. } => View::Type,
        Command::Run { .. }
        | Command::Environment {
            command: Some(EnvironmentCommand::Run { .. }),
            ..
        } => View::Run,
        Command::Environment {
            command:
                None | Some(EnvironmentCommand::Apply { .. } | EnvironmentCommand::Rollback { .. }),
            ..
        } => View::Environment,
        Command::Close { .. } => View::Close,
        Command::Open { app, targets, .. } => View::Open(
            app.iter()
                .chain(targets)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Command::Wait { gone, .. } => View::Wait { gone: *gone },
        Command::Restart => View::Restart,
        Command::StartInput => View::StartInput,
        Command::Install => View::Install,
        Command::Agents {
            action: Some(AgentsCommand::Install { .. } | AgentsCommand::Remove { .. }),
        } => View::AgentsChange,
        Command::Agents { .. } => View::Agents,
        Command::Switch { .. } => View::Switch,
        Command::Update { .. } => View::Update,
        Command::Uninstall { .. } => View::Uninstall,
        Command::Menubar { login: Some(_), .. } => View::Login,
        Command::Init | Command::Monitor { .. } | Command::Menubar { .. } => View::Quiet,
        _ => View::Generic,
    }
}

/// Write a result: JSON for programs, its view for a person. A reader that
/// went away (`| head`) ends the output quietly.
fn emit(format: Format, view: &View, value: &Value) {
    if value.is_null() {
        return;
    }
    match format {
        Format::Json => {
            let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
            let _ = writeln!(std::io::stdout().lock(), "{text}");
        }
        Format::Human => {
            let shown = render::render(view, value);
            if !shown.out.is_empty() {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(shown.out.as_bytes());
                if !shown.out.ends_with('\n') {
                    let _ = out.write_all(b"\n");
                }
                let _ = out.flush();
            }
            if !shown.err.is_empty() {
                eprintln!("{}", shown.err);
            }
        }
    }
}

/// Ctrl-C during a question or a wait puts the cursor back and says how to
/// resume, instead of leaving a half-drawn line.
fn stop_cleanly_on_interrupt(message: &'static str) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = console::Term::stderr().show_cursor();
            eprintln!("\n{} {message}", style::unknown());
            std::process::exit(130);
        }
    });
}

#[cfg(target_os = "macos")]
fn windows_info() -> Result<Value> {
    Ok(json!({ "ok": true, "windows": understudy::native::windows::windows()? }))
}
#[cfg(target_os = "macos")]
fn elements_info(pid: i32) -> Result<Value> {
    let limits = understudy::native::ax::Limits::default();
    let snapshot = understudy::native::ax::elements(pid, limits)?;
    Ok(json!({
        "ok": true,
        "elements": snapshot.elements,
        "truncated": snapshot.incomplete,
        "errors": snapshot.errors,
    }))
}
#[cfg(not(target_os = "macos"))]
fn elements_info(_: i32) -> Result<Value> {
    Err(Error::new(
        "platform",
        "Accessibility elements require macOS",
    ))
}
#[cfg(not(target_os = "macos"))]
fn windows_info() -> Result<Value> {
    Err(Error::new("platform", "Window listing requires macOS"))
}

#[cfg(target_os = "macos")]
async fn input_service(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<Value> {
    understudy::native::service::serve(uid, controller_uid, socket).await?;
    Ok(Value::Null)
}
#[cfg(target_os = "macos")]
async fn helper_serve(_: u32, _: u32, _: PathBuf) -> Result<Value> {
    unreachable!("the helper daemon is started from main")
}
#[cfg(not(target_os = "macos"))]
async fn helper_serve(_: u32, _: u32, _: PathBuf) -> Result<Value> {
    Err(Error::new("platform", "The helper daemon requires macOS"))
}
#[cfg(not(target_os = "macos"))]
async fn input_service(_: u32, _: u32, _: PathBuf) -> Result<Value> {
    Err(Error::new("platform", "The input service requires macOS"))
}

async fn execute(config: PathBuf, command: Command) -> Result<Value> {
    // Helper-side commands run inside the helper login and never read the
    // controller's configuration.
    let command = match command {
        Command::EnvironmentWorker { packet, apply } => {
            return environment::worker(&packet, apply);
        }
        Command::AgentGuide { skill, commands } => {
            if skill {
                print!("{}", agent::skill());
            } else if commands {
                print!("{}", command_reference());
            } else {
                print!("{}", agent::GUIDE);
            }
            return Ok(Value::Null);
        }
        Command::SessionInfo => return understudy::native::session_info(),
        Command::WindowsInfo => return windows_info(),
        Command::ElementsInfo { pid } => return elements_info(pid),
        Command::InputService {
            uid,
            controller_uid,
            socket,
        } => return input_service(uid, controller_uid, socket).await,
        Command::HelperServe {
            uid,
            controller_uid,
            socket,
        } => return helper_serve(uid, controller_uid, socket).await,
        Command::SetupHelper { request } => return understudy::setup::helper::run(&request),
        Command::Update {
            allow_unnotarized,
            force,
            version,
        } => return update(&config, allow_unnotarized, force, version),
        Command::UpdateFinish => return update_finish(&config),
        Command::Init => return understudy::init::run(&config).await,
        Command::Agents { action } => {
            return match action.unwrap_or(AgentsCommand::Status { brief: false }) {
                AgentsCommand::Status { brief: true } => {
                    println!("{}", understudy::agents::brief(&config).await);
                    Ok(Value::Null)
                }
                AgentsCommand::Status { brief: false } => {
                    let _spinner = Spinner::new("Checking Claude Code and Codex…");
                    Ok(understudy::agents::status())
                }
                AgentsCommand::Install { claude, codex } => {
                    let _spinner = Spinner::new("Connecting…");
                    understudy::agents::change(&chosen(claude, codex), false)
                }
                AgentsCommand::Remove { claude, codex } => {
                    let _spinner = Spinner::new("Disconnecting…");
                    understudy::agents::change(&chosen(claude, codex), true)
                }
            };
        }
        Command::Uninstall { keep_config, yes } => {
            return understudy::uninstall::run(&config, keep_config, yes).await;
        }
        Command::Menubar {
            login: Some(login), ..
        } => return menubar::login(login.as_str()),
        Command::Switch {
            account,
            lock,
            list,
            yes,
        } => return switch(&config, account, lock, list, yes),
        Command::Setup {
            helper,
            apply: false,
            rollback: false,
        } => return understudy::setup::preview(&config, helper.as_deref()).await,
        command => command,
    };
    let client = Client::new(Config::load(&config)?)?;
    match command {
        Command::InputService { .. }
        | Command::HelperServe { .. }
        | Command::SessionInfo
        | Command::WindowsInfo
        | Command::ElementsInfo { .. }
        | Command::AgentGuide { .. }
        | Command::EnvironmentWorker { .. }
        | Command::SetupHelper { .. }
        | Command::Update { .. }
        | Command::UpdateFinish
        | Command::Init
        | Command::Agents { .. }
        | Command::Uninstall { .. }
        | Command::Switch { .. } => {
            unreachable!("these commands return above")
        }
        Command::Setup { apply, .. } => {
            // Recovery ownership: the migration settles only the pending
            // mutations its own log records and refuses any other.
            let ownership = client.controller.acquire(true)?;
            if apply {
                understudy::setup::migrate::apply(&config, &ownership).await
            } else {
                understudy::setup::migrate::rollback(&config, &ownership).await
            }
        }
        Command::Config => Ok(json!({ "ok": true, "config": client.config, "path": config })),
        Command::Status => Ok(client.status().await),
        Command::Doctor { verbose } => {
            let report =
                understudy::capability::doctor(client.status().await, client.config.helper_uid);
            // People see the summary either way; programs get the full status
            // only when they ask for it.
            Ok(if verbose || ui::human() {
                report
            } else {
                understudy::capability::summary(report)
            })
        }
        Command::VerifyNative { fixture } => {
            understudy::capability::verify(&client, &fixture).await
        }
        Command::Environment {
            manifest,
            apply,
            command,
        } => match command {
            Some(EnvironmentCommand::Profile { profile, apply }) => {
                environment::profile::sync(&client, &profile, apply).await
            }
            Some(EnvironmentCommand::Run { argv }) => {
                environment::run(&client, argv).await.map(with_output)
            }
            Some(EnvironmentCommand::Apply { plan_id }) => {
                environment::apply(&client, &plan_id).await
            }
            Some(EnvironmentCommand::Rollback {
                transaction_id,
                apply,
            }) => environment::rollback(&client, &transaction_id, apply).await,
            Some(EnvironmentCommand::Discover {
                tools,
                apps,
                environment,
                references,
            }) => {
                environment::discovery::discover(&client, &tools, &apps, &environment, &references)
                    .await
            }
            None => {
                let manifest = manifest.ok_or_else(|| {
                    Error::new(
                        "usage",
                        "understudy environment needs a profile file, or a command such as discover or run.",
                    )
                })?;
                environment::sync(&client, &manifest, apply).await
            }
        },
        Command::Install => client.install(),
        Command::StartInput => client.start_input().await,
        Command::Monitor { port } => {
            viewer::serve(client, port, None).await?;
            Ok(Value::Null)
        }
        Command::Menubar {
            port,
            render,
            serve,
            ..
        } => {
            let program = menubar::Program::new(&config)?;
            match render {
                Some(directory) => viewer::render_menubar(client, &program, &directory).await,
                None if serve || menubar::started_at_login() => {
                    // Its own session: closing the terminal that started it
                    // does not hang it up.
                    if serve {
                        let _ = nix::unistd::setsid();
                    }
                    viewer::serve(client, port, Some(program)).await?;
                    Ok(Value::Null)
                }
                None => {
                    let record = client.output_dir()?.join("monitor.json");
                    menubar::start(&config, port, &record).await?;
                    Ok(Value::Null)
                }
            }
        }
        Command::Screenshot { max_dimension } => client.capture(max_dimension).await,
        Command::Apps => client.apps().await,
        Command::Click {
            x,
            y,
            target,
            name,
            role,
            button,
            count,
            modifiers,
        } => match (x, y, target.target()) {
            (Some(x), Some(y), None) => {
                act(
                    &client,
                    agent::click(x, y, button.into(), count, &modifiers),
                )
                .await
            }
            (None, None, Some(target)) => {
                let filter = agent::ElementFilter { role, name };
                agent::click_element(&client, &target, &filter, button.into(), count, &modifiers)
                    .await
            }
            _ => Err(Error::new(
                "usage",
                "Give X Y, or --app/--pid with --name/--role",
            )),
        },
        Command::Elements {
            target,
            role,
            name,
            max,
        } => {
            let target = target
                .target()
                .ok_or_else(|| Error::new("usage", "Select the app with --app or --pid"))?;
            agent::elements(&client, &target, &agent::ElementFilter { role, name }, max).await
        }
        Command::Readiness { tools, apps } => {
            understudy::readiness::readiness(&client, &tools, &apps).await
        }
        Command::Move { x, y } => act(&client, json!([{ "type": "move", "x": x, "y": y }])).await,
        Command::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
        } => {
            act(
                &client,
                agent::drag((from_x, from_y), (to_x, to_y), button.into()),
            )
            .await
        }
        Command::Scroll { x, y, dx, dy } => {
            act(
                &client,
                json!([{ "type": "scroll", "x": x, "y": y, "dx": dx, "dy": dy }]),
            )
            .await
        }
        Command::Type { text, stdin } => {
            let text = match text {
                Some(text) if !stdin => text,
                _ => {
                    const LIMIT: u64 = 1 << 20;
                    let mut text = String::new();
                    std::io::stdin().take(LIMIT + 1).read_to_string(&mut text)?;
                    if text.len() as u64 > LIMIT {
                        return Err(Error::new("input", "Text from stdin exceeds 1 MiB"));
                    }
                    text
                }
            };
            agent::type_text(&client, &text).await
        }
        Command::Key { keys } => act(&client, agent::keys(&keys)).await,
        Command::Windows { app, title, all } => {
            agent::list(&client, &agent::WindowFilter { app, title, all }).await
        }
        Command::Open {
            app,
            targets,
            environment,
        } => agent::open(&client, app.as_deref(), &targets, &environment).await,
        Command::Close { app } => agent::close(&client, &app).await,
        Command::Wait {
            app,
            title,
            gone,
            timeout,
        } => {
            let filter = agent::WindowFilter {
                app,
                title,
                all: false,
            };
            agent::wait(&client, &filter, gone, Duration::from_secs(timeout)).await
        }
        Command::Input { actions } => {
            let bytes = if let Some(path) = actions.strip_prefix('@') {
                let mut bytes = Vec::new();
                std::fs::File::open(path)?
                    .take(32769)
                    .read_to_end(&mut bytes)?;
                bytes
            } else {
                actions.into_bytes()
            };
            if bytes.len() > 32768 {
                return Err(Error::new(
                    "input",
                    "Input exceeds 32 KiB; use short observed batches",
                ));
            }
            client
                .input(&serde_json::from_slice(&bytes)?, None, None)
                .await
        }
        Command::Run { argv } => client.run(argv).await.map(with_output),
        Command::Restart => {
            let _spinner = Spinner::new("Restarting the helper…");
            client.restart().await
        }
    }
}

/// Lock the screen so the person can switch to the helper (or `account`)
/// there. macOS asks for that account's password on the lock screen; there
/// is nothing to choose here beyond whether to lock now.
fn switch(
    config: &std::path::Path,
    account: Option<String>,
    lock: bool,
    list: bool,
    yes: bool,
) -> Result<Value> {
    use understudy::switch;
    let helper = Config::load(config).ok().map(|config| config.helper_uid);
    let accounts = switch::accounts(helper)?;
    if list {
        return Ok(json!({"ok": true, "accounts": accounts}));
    }
    let target = match account {
        Some(name) => Some(
            accounts
                .iter()
                .find(|candidate| candidate.name == name)
                .cloned()
                .ok_or_else(|| {
                    Error::new("switch", format!("No account named {name} to switch to"))
                })?,
        ),
        None if lock => None,
        None => accounts.iter().find(|candidate| candidate.helper).cloned(),
    };
    if !yes {
        let question = match &target {
            Some(account) => format!(
                "Lock the screen so you can switch to {}?",
                account.real_name.as_deref().unwrap_or(&account.name)
            ),
            None => "Lock the screen now?".to_owned(),
        };
        if !ui::prompt::confirm(&question, true, "pass --yes")? {
            return Err(ui::prompt::cancelled());
        }
    }
    match target {
        Some(account) => switch::to(&account),
        None => switch::lock_screen(),
    }
}

/// Update the app, then hand over to the new executable, so the steps that
/// move the helper onto it are always the new version's own.
fn update(
    config: &std::path::Path,
    allow_unnotarized: bool,
    force: bool,
    version: Option<String>,
) -> Result<Value> {
    let source = understudy::update::Source::from_env(version);
    // Nothing else may control the helper while its app is replaced: no
    // viewer or agent, and no action of unknown outcome. The new version's
    // steps take control again one by one.
    let exclusion = match Config::load(config) {
        Ok(loaded) => Some(Client::new(loaded)?.controller.acquire(false)?),
        Err(_) => None,
    };
    let spinner = Spinner::new("Checking for an update…");
    let mut report = understudy::update::install(&source, allow_unnotarized, force)?;
    drop(exclusion);
    if report["updated"] != true {
        return Ok(report);
    }
    spinner.set("Moving the helper onto the new version…");
    let executable = understudy::update::installed_app()?.join("Contents/MacOS/understudy");
    // update-finish always answers in JSON; no flag, so a release that
    // predates --json can finish too.
    let output = std::process::Command::new(executable)
        .arg("update-finish")
        .arg("--config")
        .arg(config)
        .output()?;
    let finished: Value = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_slice(&output.stderr))
        .unwrap_or_else(|_| json!({"ok": false, "error": "The new version did not report"}));
    report["ok"] = finished["ok"].clone();
    report["helper"] = finished;
    if report["ok"] == true {
        report["previous"] = understudy::update::finished()?;
    } else {
        report["next"] = json!("Fix the failed step, then run understudy update again to resume");
    }
    Ok(report)
}

/// Move the helper onto this (just installed) executable: the shared
/// executable, the daemon, the input service, and (when a setup is
/// recorded) setup's verification of the new signed build. Stops at the
/// first failure and names it.
fn update_finish(config: &std::path::Path) -> Result<Value> {
    let Ok(loaded) = Config::load(config) else {
        return Ok(json!({"ok": true, "next": "understudy init"}));
    };
    let executable = std::env::current_exe()?;
    // The daemon restarts first: a daemon whose app was swapped under it can
    // no longer vouch (to TCC) for a new input service it starts, while the
    // input service it already runs is cleaned up by the restart itself.
    let mut steps: Vec<&[&str]> = vec![&["install"], &["restart"], &["start-input"]];
    if understudy::setup::migrate::recorded(loaded.helper_uid) {
        steps.push(&["setup", "--apply"]);
    }
    let mut done = Vec::new();
    for step in steps {
        let output = std::process::Command::new(&executable)
            .args(step)
            .arg("--json")
            .arg("--config")
            .arg(config)
            .output()?;
        // Replies go to stdout; errors to stderr.
        let reply: Value = serde_json::from_slice(&output.stdout)
            .or_else(|_| serde_json::from_slice(&output.stderr))
            .unwrap_or(Value::Null);
        if !output.status.success() || reply["ok"] == false {
            return Ok(json!({
                "ok": false,
                "failedStep": step.join(" "),
                "failure": reply,
                "completedSteps": done,
            }));
        }
        done.push(step.join(" "));
    }
    let agents = understudy::agents::refresh();
    Ok(json!({"ok": true, "completedSteps": done, "agents": agents}))
}

/// Every visible command with its full help, as Markdown: the agent
/// plugin's commands reference (a test keeps the plugin's copy equal).
fn command_reference() -> String {
    use clap::CommandFactory;
    let mut text = String::from(
        "# Understudy commands\n\nGenerated from `understudy --help`. Every command prints \
         JSON when its output is not a terminal (or with `--json`) and exits non-zero on \
         failure.\n",
    );
    let mut root = Cli::command().bin_name("understudy");
    root.build();
    // In the order of `understudy --help`.
    for name in help::GROUPS.iter().flat_map(|(_, names)| names.iter()) {
        let Some(command) = root.find_subcommand_mut(name) else {
            continue;
        };
        let help = command.render_long_help().to_string();
        text.push_str(&format!(
            "\n## understudy {name}\n\n```text\n{}\n```\n",
            help.trim_end()
        ));
    }
    text
}

#[derive(Clone, Copy, ValueEnum)]
enum LoginArg {
    On,
    Off,
    Status,
}
impl LoginArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Status => "status",
        }
    }
}

#[derive(Clone, Copy, Default, ValueEnum)]
enum ButtonArg {
    #[default]
    Left,
    Middle,
    Right,
}
impl From<ButtonArg> for Button {
    fn from(button: ButtonArg) -> Self {
        match button {
            ButtonArg::Left => Button::Left,
            ButtonArg::Middle => Button::Middle,
            ButtonArg::Right => Button::Right,
        }
    }
}

/// One guarded batch, reporting what the helper performed.
async fn act(client: &Client, actions: Value) -> Result<Value> {
    client.input(&actions, None, None).await
}

/// The helper app an element command is about.
#[derive(clap::Args)]
#[group(multiple = false)]
struct AppTarget {
    /// Application name; must resolve to one process with a window.
    #[arg(long)]
    app: Option<String>,
    /// Exact process ID, for apps with several processes.
    #[arg(long)]
    pid: Option<i64>,
}
impl AppTarget {
    fn target(self) -> Option<agent::Target> {
        self.pid
            .map(agent::Target::Pid)
            .or(self.app.map(agent::Target::Name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent plugin's command reference is exactly this CLI's help.
    #[test]
    fn the_plugin_command_reference_matches_this_cli() {
        let checked_in =
            include_str!("../../agents/understudy/skills/understudy/references/commands.md");
        assert_eq!(
            checked_in,
            command_reference(),
            "regenerate with: understudy agent-guide --commands"
        );
    }

    fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("understudy").chain(args.iter().copied()))
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
    }

    #[test]
    fn the_menu_bar_serves_or_renders() {
        let Ok(Cli {
            command: Some(Command::Menubar { port, render, .. }),
            ..
        }) = parse(&["menubar", "--render", "out"])
        else {
            panic!("menubar did not parse");
        };
        assert_eq!((port, render), (8765, Some(PathBuf::from("out"))));
        assert!(parse(&["menubar", "--port", "9000"]).is_ok());
    }

    #[test]
    fn scrolling_accepts_negative_lines() {
        let Ok(Cli {
            command: Some(Command::Scroll { dy, .. }),
            ..
        }) = parse(&["scroll", "1", "2", "--dy", "-3"])
        else {
            panic!("scroll did not parse");
        };
        assert_eq!(dy, -3);
    }
}
