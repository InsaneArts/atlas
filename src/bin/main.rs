mod help;

use atlas::{
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
use clap::{FromArgMatches, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{
    io::{IsTerminal, Read, Write},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
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
        env = "ATLAS_CONFIG",
        hide_env = true,
        help_heading = "Global options",
        global = true
    )]
    config: Option<PathBuf>,
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
    /// This process's permissions as macOS grants them now (the helper's
    /// daemon asks a fresh process: its own Screen Recording reading only
    /// changes when it restarts).
    #[command(hide = true)]
    PermissionsInfo,
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
        long_about = "Inspect or repair the helper's setup (advanced).\n\nWithout flags, report the helper account, configuration, endpoints and services without changing anything. --apply runs Atlas's own signed helper daemon as a managed LaunchAgent on a private socket directory beside the existing capture job, moves input and the configuration over once it is verified, then retires the old job, recording every step so an interruption resumes. --rollback restores the previous job and configuration."
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
    /// Show where Atlas keeps its settings, sockets and screenshots.
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
        #[arg(long, default_value = atlas::capability::FIXTURE)]
        fixture: PathBuf,
    },
    /// Put this build where the helper can run it.
    Install,
    /// Start (or reset) the helper's input service.
    StartInput,
    /// Watch and control the helper in Atlas's viewer window.
    ///
    /// Starts the menu bar first when it is not running.
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
        #[arg(long, default_value_t = 8765, value_parser = clap::value_parser!(u16).range(1..))]
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
    ///
    /// Each action is an object with a "type": move {x, y}; click {x, y,
    /// button?, count?, modifiers?}; drag {fromX, fromY, toX, toY, button?};
    /// scroll {x, y, dx?, dy?}; type {text}; key {key}; wait {ms}. Up to 32
    /// actions that finish within two seconds; coordinates are display points.
    /// Example: [{"type":"click","x":200,"y":120},{"type":"type","text":"hi"}]
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
        /// Exact element title, label text, description or identifier.
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
        /// Only elements with this exact title, label text, description or identifier.
        #[arg(long)]
        name: Option<String>,
        /// Show at most this many elements.
        #[arg(long, default_value_t = 200)]
        max: usize,
    },
    /// Print the guide agents read to use Atlas.
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
    /// Quit an app in the helper and remove the copies `open` made for it.
    ///
    /// The app quits at once, without asking to save: save first (key cmd+s).
    Close {
        /// The app's name, as `atlas apps` lists it.
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
        #[arg(last = true, required = true, value_name = "COMMAND")]
        argv: Vec<String>,
    },
    /// Restart the helper after something went wrong.
    ///
    /// Confirms a new hidden helper before clearing an action of unknown outcome.
    Restart,
    /// Lock the screen so you can switch to the helper.
    ///
    /// You sign in on the lock screen; Atlas never sees a password.
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
    /// Update Atlas, checking its signature first.
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
    /// Set Atlas up, step by step (safe to run again).
    ///
    /// Helper account, first sign-in, permissions, the helper job and input.
    Init,
    /// Remove Atlas from this Mac (asks first).
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
    /// Adds a plugin that teaches the agent to use Atlas and guides setup.
    Agents {
        #[command(subcommand)]
        action: Option<AgentsCommand>,
    },
    /// After an update, run by the new executable: move the helper onto it.
    #[command(hide = true)]
    UpdateFinish,
    /// The steps the Atlas window's onboarding drives (JSON only).
    #[command(hide = true)]
    Onboarding {
        #[command(subcommand)]
        step: OnboardingCommand,
    },
}

#[derive(Subcommand)]
enum OnboardingCommand {
    /// Everything the window needs to choose its step.
    Status,
    /// Make an existing standard account the helper.
    Choose { account: String },
    /// Create a standard account (its password on standard input) and
    /// make it the helper.
    Create {
        #[arg(long)]
        account: String,
        #[arg(long = "full-name", default_value = atlas::init::FULL_NAME)]
        full_name: String,
    },
    /// In the helper: let the account that asked use this one (the one the
    /// window showed).
    Accept {
        #[arg(long = "controller-uid")]
        controller_uid: u32,
        #[arg(long = "controller-name")]
        controller_name: String,
    },
    /// Mark the window open or closed in this account.
    Window {
        #[arg(value_parser = ["open", "closed"])]
        state: String,
    },
    /// Back in the person's account: start input and check everything.
    Finish,
    /// Open the menu bar at login (on or off), decided once.
    Login {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
}

#[derive(Subcommand)]
enum AgentsCommand {
    /// Which agents are on this Mac and whether Atlas is connected.
    Status {
        /// One plain line for an agent's session start.
        #[arg(long)]
        brief: bool,
    },
    /// Tell a project's agents to test with Atlas: adds a short section to
    /// its AGENTS.md (and to a separate CLAUDE.md).
    Project {
        /// The project folder (default: this one).
        dir: Option<PathBuf>,
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
fn chosen(claude: bool, codex: bool) -> Vec<atlas::agents::Agent> {
    use atlas::agents::Agent;
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
        #[arg(last = true, required = true, value_name = "COMMAND")]
        argv: Vec<String>,
    },
    /// Copy a previewed plan into the helper (or finish one that stopped).
    Apply {
        /// The ID the preview printed.
        #[arg(value_name = "ID")]
        plan_id: String,
    },
    /// Show what undoing a copy would do, or undo it with --apply.
    Rollback {
        /// The ID the copy printed.
        #[arg(value_name = "ID")]
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
    let config = atlas::config::effective(cli.config.take());
    // `atlas` alone shows what it can do; opened as an app (Finder,
    // Launchpad, `open`), it shows the window instead.
    let Some(command) = cli.command.take() else {
        if opened_as_app() {
            return open_app(&config);
        }
        let _ = help::command().print_help();
        return ExitCode::SUCCESS;
    };
    // Commands run by Atlas itself always answer in JSON.
    let format = if machine_only(&command) {
        Format::Json
    } else {
        ui::detect(
            cli.json,
            std::env::var("ATLAS_JSON").ok().as_deref(),
            std::io::stdout().is_terminal(),
        )
    };
    ui::set(format);
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
    let view = view(&command);
    // Commands that ask questions or wait for the person stop cleanly on
    // Ctrl-C, saying what that means for each.
    let interrupted = match command {
        Command::Init => Some("Stopped. Run atlas init again to continue."),
        Command::Switch { .. } | Command::Uninstall { .. } => Some("Stopped."),
        Command::Update { .. } => Some("Stopped. Run atlas update again to finish."),
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
                execute(config, command).await
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
                return ExitCode::from(atlas::environment::WORKER_REFUSED);
            }
            match format {
                Format::Json => eprintln!("{}", json!(error)),
                Format::Human => eprintln!("{}", render::error(&error)),
            }
            ExitCode::FAILURE
        }
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

/// Commands Atlas runs itself (in the helper, or as a step of another
/// command), whose output another Atlas process reads.
fn machine_only(command: &Command) -> bool {
    matches!(
        command,
        Command::EnvironmentWorker { .. }
            | Command::SessionInfo
            | Command::PermissionsInfo
            | Command::WindowsInfo
            | Command::ElementsInfo { .. }
            | Command::HelperServe { .. }
            | Command::InputService { .. }
            | Command::SetupHelper { .. }
            | Command::UpdateFinish
            | Command::Onboarding { .. }
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
                None
                | Some(
                    EnvironmentCommand::Apply { .. }
                    | EnvironmentCommand::Rollback { .. }
                    | EnvironmentCommand::Profile { .. },
                ),
            ..
        } => View::Environment,
        Command::Close { .. } => View::Close,
        Command::Environment {
            command: Some(EnvironmentCommand::Discover { .. }),
            ..
        } => View::Discover,
        Command::Setup {
            apply: false,
            rollback: false,
            ..
        } => View::SetupPreview,
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
        Command::Agents {
            action: Some(AgentsCommand::Project { .. }),
        } => View::AgentsProject,
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
            // An agent tool Atlas started in its own process group does not
            // see the terminal's Ctrl-C: stop it too, before saying stopped.
            atlas::agents::stop_running();
            let _ = console::Term::stderr().show_cursor();
            eprintln!("\n{} {message}", style::unknown());
            std::process::exit(130);
        }
    });
}

#[cfg(target_os = "macos")]
fn windows_info() -> Result<Value> {
    Ok(json!({ "ok": true, "windows": atlas::native::windows::windows()? }))
}
#[cfg(target_os = "macos")]
fn elements_info(pid: i32) -> Result<Value> {
    let limits = atlas::native::ax::Limits::default();
    let snapshot = atlas::native::ax::elements(pid, limits)?;
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
    atlas::native::service::serve(uid, controller_uid, socket).await?;
    Ok(Value::Null)
}
#[cfg(not(target_os = "macos"))]
async fn input_service(_: u32, _: u32, _: PathBuf) -> Result<Value> {
    Err(Error::new("platform", "The input service requires macOS"))
}

/// `atlas doctor`: the helper's readiness, with the checks only this side
/// can make (the account itself, and a leftover session of a deleted one).
async fn doctor(client: &Client, verbose: bool) -> Value {
    let uid = client.config.helper_uid;
    let mut report = atlas::capability::doctor(client.status().await, uid);
    // Both states make the helper unusable; init refuses them too.
    let mut first_steps = Vec::new();
    if let Ok(Some(account)) = atlas::setup::account_with_uid(uid) {
        if atlas::session::leftover_session(uid, &account.name, &account.home) {
            first_steps.push(format!(
                "macOS still runs a session of a deleted account with {}'s user ID and mixes it \
                 up with the helper. Restart the Mac.",
                account.name
            ));
        }
        if atlas::setup::administrator(&account.name) != Some(false) {
            first_steps.push(format!(
                "{} must be a standard account, not an administrator: change it in System \
                 Settings › Users & Groups.",
                account.name
            ));
        }
    }
    if !first_steps.is_empty() {
        report["ready"] = json!(false);
        if let Some(steps) = report["nextSteps"].as_array_mut() {
            for (index, step) in first_steps.into_iter().enumerate() {
                steps.insert(index, json!(step));
            }
        }
    }
    // People see the summary either way; programs get the full status
    // only when they ask for it.
    if verbose || ui::human() {
        report
    } else {
        atlas::capability::summary(report)
    }
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
        Command::SessionInfo => return atlas::native::session_info(),
        Command::PermissionsInfo => return Ok(atlas::daemon::facts::permissions_now()),
        Command::WindowsInfo => return windows_info(),
        Command::ElementsInfo { pid } => return elements_info(pid),
        Command::InputService {
            uid,
            controller_uid,
            socket,
        } => return input_service(uid, controller_uid, socket).await,
        Command::SetupHelper { request } => return atlas::setup::helper::run(&request),
        Command::Update {
            allow_unnotarized,
            force,
            version,
        } => return update(&config, allow_unnotarized, force, version),
        Command::UpdateFinish => return update_finish(&config),
        Command::Init => return atlas::init::run(&config).await,
        Command::Onboarding { step } => {
            use atlas::onboarding;
            return match step {
                OnboardingCommand::Status => onboarding::status(&config).await,
                OnboardingCommand::Choose { account } => onboarding::choose(&config, &account),
                OnboardingCommand::Create { account, full_name } => {
                    onboarding::create(&config, &account, &full_name)
                }
                OnboardingCommand::Accept {
                    controller_uid,
                    controller_name,
                } => onboarding::accept(controller_uid, &controller_name),
                OnboardingCommand::Window { state } => onboarding::window(state == "open"),
                OnboardingCommand::Finish => onboarding::finish(&config).await,
                OnboardingCommand::Login { state } => onboarding::login(&config, state == "on"),
            };
        }
        Command::Agents { action } => {
            return match action.unwrap_or(AgentsCommand::Status { brief: false }) {
                AgentsCommand::Status { brief: true } => {
                    println!("{}", atlas::agents::brief(&config).await);
                    Ok(Value::Null)
                }
                AgentsCommand::Status { brief: false } => {
                    let _spinner = Spinner::new("Checking Claude Code and Codex…");
                    Ok(atlas::agents::status())
                }
                AgentsCommand::Project { dir } => {
                    atlas::agents::project(&dir.unwrap_or_else(|| PathBuf::from(".")))
                }
                AgentsCommand::Install { claude, codex } => {
                    let _spinner = Spinner::new("Connecting…");
                    atlas::agents::change(&chosen(claude, codex), false)
                }
                AgentsCommand::Remove { claude, codex } => {
                    let _spinner = Spinner::new("Disconnecting…");
                    atlas::agents::change(&chosen(claude, codex), true)
                }
            };
        }
        Command::Uninstall { keep_config, yes } => {
            return atlas::uninstall::run(&config, keep_config, yes).await;
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
        } => return atlas::setup::preview(&config, helper.as_deref()).await,
        command => command,
    };
    let client = Client::new(Config::load(&config)?)?;
    match command {
        Command::InputService { .. }
        | Command::HelperServe { .. }
        | Command::SessionInfo
        | Command::PermissionsInfo
        | Command::WindowsInfo
        | Command::ElementsInfo { .. }
        | Command::AgentGuide { .. }
        | Command::EnvironmentWorker { .. }
        | Command::SetupHelper { .. }
        | Command::Update { .. }
        | Command::UpdateFinish
        | Command::Init
        | Command::Onboarding { .. }
        | Command::Agents { .. }
        | Command::Uninstall { .. }
        | Command::Switch { .. } => {
            unreachable!("these commands return above, or from main")
        }
        Command::Setup { apply, .. } => {
            // Recovery ownership: the migration settles only the pending
            // mutations its own log records and refuses any other.
            let ownership = client.controller.acquire(true)?;
            if apply {
                atlas::setup::migrate::apply(&config, &ownership).await
            } else {
                atlas::setup::migrate::rollback(&config, &ownership).await
            }
        }
        Command::Config => Ok(json!({ "ok": true, "config": client.config, "path": config })),
        Command::Status => Ok(client.status().await),
        Command::Doctor { verbose } => Ok(doctor(&client, verbose).await),
        Command::VerifyNative { fixture } => atlas::capability::verify(&client, &fixture).await,
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
                        "atlas environment needs a profile file, or a command such as discover or run.",
                    )
                })?;
                environment::sync(&client, &manifest, apply).await
            }
        },
        Command::Install => client.install(),
        Command::StartInput => client.start_input().await,
        Command::Monitor { port } => {
            viewer::monitor(&config, port, client.output_dir()?).await?;
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
                    viewer::serve(client, port, program).await?;
                    Ok(Value::Null)
                }
                None => {
                    let output = client.output_dir()?.to_path_buf();
                    menubar::start(&config, port, &output).await?;
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
            atlas::readiness::readiness(&client, &tools, &apps).await
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
        Command::Key { keys } => act(&client, agent::keys(&keys)?).await,
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
    use atlas::switch;
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
    let source = atlas::update::Source::from_env(version);
    // Nothing else may control the helper while its app is replaced: no
    // viewer or agent, and no action of unknown outcome. The new version's
    // steps take control again one by one.
    let exclusion = match Config::load(config) {
        Ok(loaded) => Some(Client::new(loaded)?.controller.acquire(false)?),
        Err(_) => None,
    };
    let spinner = Spinner::new("Checking for an update…");
    let mut report = atlas::update::install(&source, allow_unnotarized, force)?;
    drop(exclusion);
    if report["updated"] != true {
        return Ok(report);
    }
    spinner.set("Moving the helper onto the new version…");
    let executable = atlas::update::installed_executable()?;
    // update-finish always answers in JSON; no flag, so a release that
    // predates --json can finish too.
    let output = std::process::Command::new(&executable)
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
        report["previous"] = atlas::update::finished()?;
        // A running menu bar moves onto the new version too.
        let output = Config::load(config)
            .ok()
            .and_then(|loaded| Client::new(loaded).ok())
            .and_then(|client| client.output_dir().ok().map(std::path::Path::to_path_buf));
        if let Some(port) = output.and_then(|output| atlas::menubar::stop(&output)) {
            spinner.set("Restarting the menu bar…");
            let restarted = std::process::Command::new(&executable)
                .arg("menubar")
                .arg("--port")
                .arg(port.to_string())
                .arg("--config")
                .arg(config)
                .env("ATLAS_JSON", "1")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            report["menubarRestarted"] = json!(restarted);
        }
    } else {
        report["next"] = json!("Fix the failed step, then run atlas update again to resume");
    }
    Ok(report)
}

/// Move the helper onto this (just installed) executable: the shared
/// executable, the daemon, the input service, and (when a setup is
/// recorded) setup's verification of the new signed build. Stops at the
/// first failure and names it.
fn update_finish(config: &std::path::Path) -> Result<Value> {
    let Ok(loaded) = Config::load(config) else {
        return Ok(json!({"ok": true, "next": "atlas init"}));
    };
    let executable = std::env::current_exe()?;
    // The daemon restarts first: a daemon whose app was swapped under it can
    // no longer vouch (to TCC) for a new input service it starts, while the
    // input service it already runs is cleaned up by the restart itself.
    let mut steps: Vec<&[&str]> = vec![&["install"], &["restart"], &["start-input"]];
    if atlas::setup::migrate::recorded(loaded.helper_uid) {
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
    let agents = atlas::agents::refresh();
    Ok(json!({"ok": true, "completedSteps": done, "agents": agents}))
}

/// Every visible command with its full help, as Markdown: the agent
/// plugin's commands reference (a test keeps the plugin's copy equal).
fn command_reference() -> String {
    let mut text = String::from(
        "# Atlas commands\n\nGenerated from `atlas --help`. Every command prints \
         JSON when its output is not a terminal (or with `--json`) and exits non-zero on \
         failure.\n",
    );
    // With each command's examples, as `--help` shows them.
    let mut root = help::command().bin_name("atlas");
    root.build();
    // In the order of `atlas --help`.
    for name in help::GROUPS.iter().flat_map(|(_, names)| names.iter()) {
        let Some(command) = root.find_subcommand_mut(name) else {
            continue;
        };
        let help = command.render_long_help().to_string();
        text.push_str(&format!(
            "\n## atlas {name}\n\n```text\n{}\n```\n",
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
        let checked_in = include_str!("../../agents/atlas/skills/atlas/references/commands.md");
        assert_eq!(
            checked_in,
            command_reference(),
            "regenerate with: atlas agent-guide --commands"
        );
    }

    fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("atlas").chain(args.iter().copied()))
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
