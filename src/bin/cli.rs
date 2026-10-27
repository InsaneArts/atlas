//! Command-line grammar, value parsing and argument adapters.
use atlas::{actions::Button, agent};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "A hidden second macOS desktop where agents test apps while you keep working"
)]
pub(super) struct Cli {
    /// Print JSON (the default when the output is not a terminal).
    #[arg(long, global = true, help_heading = "Global options")]
    pub(super) json: bool,
    /// Use another settings file.
    #[arg(
        long,
        value_name = "FILE",
        env = "ATLAS_CONFIG",
        hide_env = true,
        help_heading = "Global options",
        global = true
    )]
    pub(super) config: Option<PathBuf>,
    #[command(subcommand)]
    pub(super) command: Option<Command>,
}

#[derive(Subcommand)]
pub(super) enum Command {
    /// Copy selected settings and files into the helper.
    #[command(
        args_conflicts_with_subcommands = true,
        long_about = "Copy selected settings and files into the helper.\n\nPreview a one-way copy into the helper. The TOML manifest lists [[files]] tables: source (absolute or relative to the manifest), target (an ASCII path inside the helper's home) and optional executable. Quit apps that write these files before applying. A different file already in the helper, or one changed there after Atlas copied it, is a conflict; a target inside, around or differing only in case from another, or from a file copied earlier, is refused. Rollback undoes it, leaves a file changed in the helper since as it is, and deletes the copies Atlas kept for it. Accounts, sign-ins and running apps' databases are never copied."
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
    SetupHelper {
        request: String,
        /// Refuse unless this account matches this JSON account identity.
        #[arg(long)]
        account: String,
        /// Refuse unless this account is logged out.
        #[arg(long)]
        logged_out: bool,
    },
    /// Show where Atlas keeps its settings, sockets and screenshots.
    Config,
    /// Show the helper: who it is, where it is and what it may do.
    Status,
    /// Check that everything is ready, and say what to fix if not.
    Doctor {
        /// Include every check and the helper's full status (in --json too).
        #[arg(long)]
        verbose: bool,
        /// Show how long the last login of the past two hours took, step by step.
        #[arg(long, conflicts_with = "verbose")]
        login_timing: bool,
    },
    /// Check that native Save and Open dialogs work in the helper.
    ///
    /// Exercises real Save/Open dialogs with the AppKit fixture and records the outcome.
    VerifyNative {
        /// The compiled DesktopFixture test app, where the helper can run it.
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
    /// Stop agents in the helper until the person resumes them.
    #[command(hide = true)]
    Stop,
    /// Let agents act in the helper again: the person's, from a viewer
    /// (Atlas.app, the Claude Code pane); refused for an agent.
    #[command(hide = true)]
    Resume,
    /// Serve agent apps over MCP on standard input and output: the status,
    /// and a live view of the helper's screen with Stop.
    #[command(hide = true)]
    Mcp {
        /// Local port of the menu bar's viewer.
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// Offer only the status and the screen: no Stop, Resume or Recover.
        #[arg(long)]
        read_only: bool,
        /// Require a private view capability for person-operated MCP actions.
        #[arg(long, hide = true)]
        guard_app_controls: bool,
    },
    /// The helper's status as it changes, one JSON line each, for agent apps.
    #[command(hide = true)]
    Watch {
        /// Local port of the menu bar's viewer.
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// A private folder to keep the helper's newest screen in.
        #[arg(long)]
        frames: Option<std::path::PathBuf>,
        /// Say the status once and end.
        #[arg(long)]
        once: bool,
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
    ///
    /// Lists apps with a Dock icon and menu bar apps that are not part of
    /// macOS; `more` says how many other processes were left out.
    Apps {
        /// List every process, macOS's own background ones too.
        #[arg(long)]
        all: bool,
    },
    /// Send several actions at once (a JSON array, or @path/to/actions.json).
    ///
    /// Each action is an object with a "type": move {x, y}; click {x, y,
    /// button?, count?, modifiers?}; drag {fromX, fromY, toX, toY, button?};
    /// scroll {x, y, dx?, dy?}; type {text}; key {key}; wait {ms}. Up to 32
    /// actions that finish within two seconds; coordinates are display points,
    /// a click is 1 to 5 clicks, a scroll at most 100 lines, text at most 256
    /// characters, a wait at most 250 ms.
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
        /// Clicks in a row, 1 to 5: 2 for a double click.
        #[arg(long, default_value_t = 1)]
        count: u8,
        /// Comma-separated: cmd, ctrl, alt, shift, fn.
        #[arg(long, value_delimiter = ',')]
        modifiers: Vec<String>,
    },
    /// List an app's buttons, fields and other elements.
    ///
    /// Lists accessibility elements of the app's windows: role, name, value,
    /// centre point (at) and size, with each window listed once; also the
    /// items of a menu the app has open (menu: true, with a check mark as
    /// mark) and, while the app is in front, its menu bar titles. more says
    /// how many matching elements were left out.
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
        #[arg(long, default_value_t = 50)]
        max: usize,
        /// Show every attribute the scan read, as it read it.
        #[arg(long)]
        full: bool,
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
    ///
    /// What is dragged (a window, a slider's knob, a selection) moves by
    /// exactly the distance between the points; the button itself is released
    /// up to one point past the end, in the direction of travel, never off the
    /// display. Drag-and-drop onto a target (selected text, files, list items)
    /// does not begin in the helper: macOS starts such a drag only from a real
    /// mouse.
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
    /// Scroll at X Y by lines, as a mouse wheel's notches (about 10 points
    /// each, as the app has it); negative values scroll up or left.
    Scroll {
        /// Display point X.
        x: f64,
        /// Display point Y.
        y: f64,
        /// Lines to scroll sideways, at most 100; negative scrolls left.
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        dx: i32,
        /// Lines to scroll, at most 100; negative scrolls up.
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        dy: i32,
    },
    /// Type text.
    ///
    /// The text is typed as given: a real newline or tab in it (in a shell,
    /// $'one\ntwo') or on standard input arrives as a line break or tab,
    /// while the two characters \n are typed as they are. Long text is typed
    /// in parts without breaking characters, and Atlas keeps control until the
    /// last part, so no other input lands in between.
    Type {
        /// The text to type.
        #[arg(required_unless_present = "stdin", allow_hyphen_values = true)]
        text: Option<String>,
        /// Read the text from standard input instead.
        #[arg(long, conflicts_with = "text")]
        stdin: bool,
    },
    /// Press keys, for example cmd+s or escape.
    ///
    /// Keys are pressed in order: `key cmd+s` or `key escape return`. A key is
    /// a letter, digit or punctuation mark, or one of return, tab, space,
    /// escape, backspace, forwarddelete, left, right, up, down, home, end,
    /// pageup, pagedown and f1 to f12; the plus key is written + on its own or
    /// at the end of a chord (cmd++).
    Key {
        /// Keys or chords such as cmd+s, escape, return, shift+tab.
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// List the helper's windows, front to back.
    ///
    /// Every app's windows, floating panels and alerts. --all adds menus,
    /// the menu bar, the Dock and other system layers.
    Windows {
        /// Only windows of this app.
        #[arg(long)]
        app: Option<String>,
        /// Only windows whose title contains this.
        #[arg(long)]
        title: Option<String>,
        /// Include menus, the menu bar, the Dock and other system layers.
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
    /// --all quits what an earlier session may have left open: every app
    /// bundle with a Dock icon and every menu bar app not made by Apple.
    /// Atlas and macOS's own in /System/Library (Finder among them) stay.
    #[command(group(clap::ArgGroup::new("which").required(true).args(["app", "all"])))]
    Close {
        /// The app's name, as `atlas apps` lists it.
        #[arg(long)]
        app: Option<String>,
        /// Quit every app left open (not Atlas or macOS's own).
        #[arg(long, conflicts_with = "app")]
        all: bool,
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
        /// Seconds, at most 86400 (a day).
        #[arg(long, default_value_t = 10, value_parser = wait_seconds)]
        timeout: u64,
    },
    /// Run a command in the helper and print its output.
    ///
    /// The command starts in the helper's home and finds programs where
    /// readiness looks for them: the helper's .local/bin and .cargo/bin,
    /// Homebrew, /usr/local/bin and the system folders. It is stopped after
    /// --timeout seconds (20 unless given, at most 600). Start anything
    /// longer in the background and read its log later. Atlas exits 1 when
    /// the command's exit code is not 0 or it was stopped, with the reply
    /// still ok.
    Run {
        /// Seconds before the command is stopped, 1 to 600.
        #[arg(long, value_name = "SECONDS", default_value_t = atlas::client::RUN_TIMEOUT.as_secs(), value_parser = run_seconds)]
        timeout: u64,
        /// The program and its arguments, after `--`.
        #[arg(last = true, required = true, value_name = "COMMAND")]
        argv: Vec<String>,
    },
    /// Restart the helper after something went wrong.
    ///
    /// Confirms a new hidden helper before clearing an action of unknown outcome.
    Restart,
    /// Log the helper out.
    ///
    /// Its apps quit as at any log-out; one asking to save holds it up. --force ends the session anyway. You log in to the helper again to use it.
    Logout {
        /// End the helper's session without asking its apps.
        #[arg(long)]
        force: bool,
        /// Log out without asking first.
        #[arg(long)]
        yes: bool,
    },
    /// Lock the screen so you can switch to the helper.
    ///
    /// You log in on the lock screen; Atlas never sees a password.
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
    /// Verifies the release's checksum, Developer ID signature and version, then moves the helper onto it.
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
        /// Only say whether a newer release is out; install nothing.
        #[arg(long, conflicts_with_all = ["allow_unnotarized", "force"])]
        check: bool,
    },
    /// Set Atlas up, step by step (safe to run again).
    ///
    /// Helper account, first sign-in, permissions, the helper job and input.
    Init,
    /// Remove Atlas from this Mac (asks first).
    ///
    /// The helper is logged out first. Its account and files stay unless you pass --remove-helper; the app goes to the Trash.
    Uninstall {
        /// Keep configuration, setup records and logs.
        #[arg(long)]
        keep_config: bool,
        /// Do not ask.
        #[arg(long)]
        yes: bool,
        /// Also delete the helper account and its files, once it is logged out.
        #[arg(long)]
        remove_helper: bool,
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
pub(super) enum OnboardingCommand {
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
    /// Serve the unprivileged setup connection after Allow.
    #[command(hide = true)]
    AcceptWorker {
        #[arg(long)]
        ticket: String,
    },
    /// Mark the window open or closed in this account.
    Window {
        #[arg(value_enum)]
        state: WindowState,
    },
    /// Take back this account's request of a helper that never answered.
    Forget,
    /// Close the logins deleted accounts left open (macOS asks to approve).
    Close {
        /// Log out the helper instead, where Atlas isn't answering.
        #[arg(long)]
        helper: bool,
    },
    /// Back in the person's account: start input and check everything.
    Finish,
    /// Open the menu bar at login (on or off), decided once.
    Login {
        #[arg(value_enum)]
        state: OnOff,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum WindowState {
    Open,
    Closed,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(super) enum OnOff {
    On,
    Off,
}

#[derive(Subcommand)]
pub(super) enum AgentsCommand {
    /// Which agents are on this Mac and whether Atlas is connected.
    ///
    /// JSON keeps Claude Code (`claude`) and the Claude app (`claude-app`)
    /// as separate connection parts. Atlas.app groups them into one Claude
    /// row, connected only when every installed part is connected.
    Status {
        /// One plain line for an agent's session start.
        #[arg(long)]
        brief: bool,
    },
    /// Tell a project's agents to test with Atlas: adds a short section to
    /// its AGENTS.md, and to a CLAUDE.md it has that does not include AGENTS.md.
    Project {
        /// The project folder (default: this one).
        dir: Option<PathBuf>,
    },
    /// Take the Atlas section out of a project's AGENTS.md and CLAUDE.md.
    Forget {
        /// The project folder (default: this one).
        dir: Option<PathBuf>,
    },
    /// Cursor's internal command-hook adapter.
    #[command(hide = true)]
    CursorHook {
        #[arg(value_enum)]
        event: CursorHookEvent,
    },
    /// Connect Atlas to coding agents (all installed by default).
    ///
    /// Use --claude --app together to connect both Claude parts. Select only
    /// the parts installed on this Mac. JSON reports each part's result by id.
    Install {
        /// Only Claude Code.
        #[arg(long)]
        claude: bool,
        /// Only Codex.
        #[arg(long)]
        codex: bool,
        /// Only opencode.
        #[arg(long)]
        opencode: bool,
        /// Only Pi.
        #[arg(long)]
        pi: bool,
        /// Only Cursor.
        #[arg(long)]
        cursor: bool,
        /// Only the Claude app.
        #[arg(long)]
        app: bool,
    },
    /// Disconnect Atlas from coding agents.
    ///
    /// Use --claude --app together to disconnect both Claude parts.
    Remove {
        /// Only Claude Code.
        #[arg(long)]
        claude: bool,
        /// Only Codex.
        #[arg(long)]
        codex: bool,
        /// Only opencode.
        #[arg(long)]
        opencode: bool,
        /// Only Pi.
        #[arg(long)]
        pi: bool,
        /// Only Cursor.
        #[arg(long)]
        cursor: bool,
        /// Only the Claude app.
        #[arg(long)]
        app: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum CursorHookEvent {
    SessionStart,
    BeforeShell,
}

#[derive(Subcommand)]
pub(super) enum EnvironmentCommand {
    /// Freeze selected files, exported variables and readiness requirements.
    Profile {
        /// TOML profile (version = 2) of files, tools, applications, environment and references.
        profile: PathBuf,
        /// Apply it; otherwise only preview.
        #[arg(long)]
        apply: bool,
    },
    /// Run a command with the selected helper environment and helper tool paths.
    ///
    /// It is stopped after --timeout seconds (20 unless given, at most 600).
    Run {
        /// Seconds before the command is stopped, 1 to 600.
        #[arg(long, value_name = "SECONDS", default_value_t = atlas::client::RUN_TIMEOUT.as_secs(), value_parser = run_seconds)]
        timeout: u64,
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
        /// Undo the copy; otherwise show what undoing it would do.
        #[arg(long)]
        apply: bool,
    },
    /// Inspect selected tools, apps, variables and reference paths without launching them.
    Discover {
        /// A tool by its bare command name, such as cat.
        #[arg(long = "tool")]
        tools: Vec<String>,
        /// An app by its name, such as Safari.
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

/// `run --timeout`: whole seconds the helper allows a command.
fn run_seconds(value: &str) -> std::result::Result<u64, String> {
    value
        .parse()
        .ok()
        .filter(|seconds| (1..=atlas::client::RUN_TIMEOUT_MAX.as_secs()).contains(seconds))
        .ok_or_else(|| {
            format!(
                "Give 1 to {} seconds.",
                atlas::client::RUN_TIMEOUT_MAX.as_secs()
            )
        })
}

/// The longest `wait --timeout`: a day.
const WAIT_MAX_SECONDS: u64 = 24 * 60 * 60;

/// `wait --timeout`: whole seconds, 0 for one look.
fn wait_seconds(value: &str) -> std::result::Result<u64, String> {
    value
        .parse()
        .ok()
        .filter(|seconds| *seconds <= WAIT_MAX_SECONDS)
        .ok_or_else(|| format!("Give 0 to {WAIT_MAX_SECONDS} seconds."))
}

#[derive(Clone, Copy, ValueEnum)]
pub(super) enum LoginArg {
    On,
    Off,
    Status,
}

impl LoginArg {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Status => "status",
        }
    }
}

#[derive(Clone, Copy, Default, ValueEnum)]
pub(super) enum ButtonArg {
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

/// The helper app an element command is about.
#[derive(clap::Args)]
#[group(multiple = false)]
pub(super) struct AppTarget {
    /// Application name: the one process of that name with a window, or
    /// the app in front.
    #[arg(long)]
    pub(super) app: Option<String>,
    /// Exact process ID, for apps with several processes.
    #[arg(long)]
    pub(super) pid: Option<i32>,
}

impl AppTarget {
    pub(super) fn target(self) -> Option<agent::Target> {
        self.pid
            .map(|pid| agent::Target::Pid(pid.into()))
            .or(self.app.map(agent::Target::Name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("atlas").chain(args.iter().copied()))
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

    /// Text and keys that begin with a hyphen are typed and pressed, not
    /// read as options.

    #[test]
    fn text_may_begin_with_a_hyphen_and_keys_may_be_one() {
        let Ok(Cli {
            command: Some(Command::Type { text, stdin }),
            ..
        }) = parse(&["type", "-hi there"])
        else {
            panic!("type did not parse");
        };

        assert_eq!((text.as_deref(), stdin), (Some("-hi there"), false));

        let Ok(Cli {
            command: Some(Command::Key { keys }),
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
                    command: Some(Command::Key { .. }),
                    ..
                })
            ),
            "an option after the keys stays an option"
        );
        assert!(matches!(
            parse(&["type", "--stdin"]),
            Ok(Cli {
                command: Some(Command::Type {
                    text: None,
                    stdin: true
                }),
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
    fn an_environment_command_takes_the_same_time_limit_as_run() {
        let given = |args: &[&str]| match parse(args) {
            Ok(Cli {
                command:
                    Some(Command::Environment {
                        command: Some(EnvironmentCommand::Run { timeout, .. }),
                        ..
                    }),
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
}
