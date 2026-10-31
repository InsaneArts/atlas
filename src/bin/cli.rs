//! Command-line grammar, value parsing and argument adapters.
use atlas::{
    actions::Button,
    agent,
    control::{CONTROL_WAIT, CONTROL_WAIT_MAX},
    daemon::protocol::{JobId, Region},
    host_session::Key,
};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum, builder::TypedValueParser};
use std::{ffi::OsStr, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "A hidden second macOS desktop where agents test apps while you keep working"
)]
pub(super) struct Cli {
    /// Print JSON (the default when the output is not a terminal).
    #[arg(long, global = true, help_heading = "Global options")]
    pub(super) json: bool,
    /// Permit service restarts while other sessions or background jobs are active.
    /// A controller or an unfinished action can still block the restart.
    #[arg(long, global = true, help_heading = "Global options")]
    pub(super) allow_disrupt: bool,
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
    /// Seconds a command that changes the helper waits for another Atlas
    /// command to finish before answering busy, in place of `ATLAS_WAIT`: 30
    /// unless given, 0 answers at once, at most 600.
    #[arg(
        long,
        global = true,
        value_name = "SECONDS",
        hide = true,
        value_parser = control_wait
    )]
    pub(super) wait: Option<Duration>,
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
    WindowsInfo {
        /// Also say where the Dock takes clicks, read through Accessibility.
        #[arg(long)]
        dock: bool,
    },
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
    /// List what this agent session did in the helper.
    ///
    /// Its recent actions, its screenshots, the copies open made that are
    /// still there, the environment plans it applied and not yet rolled
    /// back, whether an action of its own is unknown, and each task it
    /// named with ATLAS_TASK=NAME. Claude Code, Codex, opencode and Pi name
    /// their session themselves; elsewhere set ATLAS_SESSION=NAME. Without a
    /// session, or with --all, every session's. It only reads: it works
    /// while another agent or the person has control, or after a Stop.
    Activity {
        /// Every session's, not only this one's.
        #[arg(long, conflicts_with = "session")]
        all: bool,
        /// Another session's, by the key --all lists.
        #[arg(long, value_name = "KEY", value_parser = session_key)]
        session: Option<Key>,
        /// How many recent actions to list: 10 unless given, at most 50.
        #[arg(
            long,
            value_name = "N",
            default_value_t = 10,
            value_parser = clap::value_parser!(u16).range(1..=50)
        )]
        limit: u16,
    },
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
    /// Save a screenshot of the helper's screen, an app's windows or a region.
    ///
    /// Reports origin and pointsPerPixel: the image's pixel (px, py) is the
    /// display point (origin.x + px × pointsPerPixel, origin.y + py ×
    /// pointsPerPixel). With --app the picture holds the app's windows, its
    /// sheets, panels and open menus with a small margin; overlapping lists other
    /// apps' windows in front of them (take a full screenshot to see those).
    #[command(visible_alias = "capture")]
    Screenshot {
        #[command(flatten)]
        target: AppTarget,
        /// Only this part of the display: X,Y,W,H in display points, widened to
        /// whole points.
        #[arg(
            long,
            value_name = "X,Y,W,H",
            allow_hyphen_values = true,
            value_parser = region,
            conflicts_with_all = ["app", "pid"]
        )]
        region: Option<Region>,
        /// Shrink the longer side to at most this many pixels; 0 keeps the
        /// full size. Without it: at most 1456 and one pixel per display point.
        #[arg(long, value_parser = clap::value_parser!(u32).range(0..=8192))]
        max_dimension: Option<u32>,
        /// Save a new file even when nothing changed since the last screenshot.
        #[arg(long)]
        new_file: bool,
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
    /// characters, a wait at most 250 ms. With --app or --pid, each action is
    /// checked before it starts, as type, key, click, scroll and drag check it,
    /// and a move at its point.
    /// Example: [{"type":"click","x":200,"y":120},{"type":"type","text":"hi"}]
    Input {
        /// A JSON array of actions, or @path/to/actions.json.
        actions: String,
        #[command(flatten)]
        target: AppTarget,
    },
    /// Click a point, or exactly one element of an app.
    ///
    /// Click at display point X Y (origin + screenshot pixels × pointsPerPixel),
    /// or on exactly one accessibility element of an app with --app and
    /// --name/--role. Named input brings its target forward, then checks
    /// the same point before clicking. With X Y, --app or --pid selects the
    /// app, and --window selects a particular window. A blocked or unreadable
    /// target refuses the click. No app is launched.
    Click {
        /// Display point X (origin + screenshot pixels × pointsPerPixel).
        #[arg(requires = "y")]
        x: Option<f64>,
        /// Display point Y.
        y: Option<f64>,
        #[command(flatten)]
        target: AppTarget,
        /// Bring window N (from atlas windows) forward and check the point.
        /// Nothing is sent if it is still covered or unreadable.
        #[arg(long, value_name = "N", requires = "x")]
        window: Option<i64>,
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
    /// click point (at) and size, with each window listed once; also the
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
    /// Print the guide agents read to use Atlas (all of it by default).
    #[command(group(clap::ArgGroup::new("part").args(["skill", "commands", "errors"])))]
    AgentGuide {
        /// Print the short version the agent plugins load (SKILL.md), which lists the rest.
        #[arg(long)]
        skill: bool,
        /// Print every command and its options (Markdown).
        #[arg(long)]
        commands: bool,
        /// Print what each error code means and what to do (Markdown).
        #[arg(long)]
        errors: bool,
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
    /// mouse. With --app or --pid, bring the target forward, then check that
    /// app at both ends. --window selects the starting window. A blocked or
    /// unreadable target refuses the drag; no app is launched.
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
        #[command(flatten)]
        target: AppTarget,
        /// Only if window N (from atlas windows) is the top one at the
        /// start point. Nothing is sent otherwise.
        #[arg(long, value_name = "N")]
        window: Option<i64>,
    },
    /// Scroll at a point.
    ///
    /// Scroll at X Y by lines, as a mouse wheel's notches (about 10 points
    /// each, as the app has it); negative values scroll up or left. With
    /// --app or --pid, bring the target forward and check the same point;
    /// --window selects a particular window. A blocked or unreadable target
    /// refuses the scroll. No app is launched.
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
        #[command(flatten)]
        target: AppTarget,
        /// Bring window N (from atlas windows) forward and check the point.
        /// Nothing is sent if it is still covered or unreadable.
        #[arg(long, value_name = "N")]
        window: Option<i64>,
    },
    /// Type text.
    ///
    /// The text is typed as given: a real newline or tab in it (in a shell,
    /// $'one\ntwo') or on standard input arrives as a line break or tab,
    /// while the two characters \n are typed as they are. Long text is typed
    /// in parts without breaking characters, and Atlas keeps control until the
    /// last part, so no other input lands in between. With --app or --pid,
    /// bring the app forward; --window selects a particular document and
    /// --field establishes its named field where supported. Check focus before
    /// every 16 characters. A blocked or unreadable target refuses the text.
    /// No app is launched.
    Type {
        /// The text to type.
        #[arg(required_unless_present = "stdin", allow_hyphen_values = true)]
        text: Option<String>,
        /// Read the text from standard input instead.
        #[arg(long, conflicts_with = "text")]
        stdin: bool,
        #[command(flatten)]
        target: AppTarget,
        /// Bring window N (from atlas windows) forward; refuse if a sheet
        /// blocks it or its focus cannot be proved.
        #[arg(long, value_name = "N")]
        window: Option<i64>,
        /// Establish a named field where supported, as atlas elements
        /// --name matches it, then check its focus before sending.
        #[arg(long, value_name = "NAME")]
        field: Option<String>,
    },
    /// Press keys, for example cmd+s or escape.
    ///
    /// Keys are pressed in order: `key cmd+s` or `key escape return`. A key is
    /// a letter, digit or punctuation mark, or one of return, tab, space,
    /// escape, backspace, forwarddelete, left, right, up, down, home, end,
    /// pageup, pagedown and f1 to f12; the plus key is written + on its own or
    /// at the end of a chord (cmd++). Named input brings the target forward
    /// and checks it before the first key. Later keys only check the app,
    /// since a key may move the window or field on purpose. A blocked or
    /// unreadable target refuses input. No app is launched.
    Key {
        /// Keys or chords such as cmd+s, escape, return, shift+tab.
        #[arg(required = true)]
        keys: Vec<String>,
        #[command(flatten)]
        target: AppTarget,
        /// Bring window N (from atlas windows) forward; refuse if a sheet
        /// blocks it or its focus cannot be proved.
        #[arg(long, value_name = "N")]
        window: Option<i64>,
        /// Establish a named field where supported, as atlas elements
        /// --name matches it, then check its focus before sending.
        #[arg(long, value_name = "NAME")]
        field: Option<String>,
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
    /// --all quits every app left open, another agent's too: use it only when
    /// the person asks. Atlas and macOS's own in /System/Library (Finder among
    /// them) stay.
    #[command(group(clap::ArgGroup::new("which").required(true).args(["app", "all"])))]
    Close {
        /// The app's name, as `atlas apps` lists it.
        #[arg(long)]
        app: Option<String>,
        /// Quit every app left open, another agent's too (not Atlas or macOS's own).
        #[arg(long, conflicts_with = "app")]
        all: bool,
    },
    /// Install an app into the helper's Applications folder for this task.
    ///
    /// Copies an .app from this Mac into ~/Applications in the helper, never
    /// over anything already there, registers it so atlas open --app NAME
    /// finds it, and records it, so that atlas cleanup --apply moves it aside
    /// in the helper once the task is done, unless it changed meanwhile.
    /// Settings and data the app writes in the helper stay.
    InstallApp {
        /// The .app to install.
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
    /// See what this task left in the helper, and clean it up with --apply.
    ///
    /// Lists the copies into the helper this task applied, the apps it
    /// installed, the copies it opened and the apps running from them. With
    /// --apply it quits those apps, rolls the copies into the helper back
    /// newest first, moves the installs aside in the helper (held, and removed
    /// after 7 days), removes the copies, and reports what it kept and why.
    /// Run it again to finish what is left. Apps opened by name that still run
    /// are listed, not quit.
    Cleanup {
        /// Do it; otherwise only show what would be done.
        #[arg(long)]
        apply: bool,
        /// An ended session's work, by the key atlas activity --all shows.
        #[arg(long, value_name = "KEY", value_parser = session_key)]
        session: Option<Key>,
        /// Only one task, by the key atlas activity shows for it.
        #[arg(long, value_name = "KEY", value_parser = session_key)]
        task: Option<Key>,
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
    /// --timeout seconds (20 unless given, at most 600). Other agents may
    /// act while it runs; recovery, restart and updates answer busy until
    /// it ends. Atlas exits 1 when the command's exit code is not 0 or it
    /// was stopped, with the reply still ok. For a server or worker that
    /// must keep running, use atlas job start.
    Run {
        /// Seconds before the command is stopped, 1 to 600.
        #[arg(long, value_name = "SECONDS", default_value_t = atlas::client::RUN_TIMEOUT.as_secs(), value_parser = run_seconds)]
        timeout: u64,
        /// The program and its arguments, after `--`.
        #[arg(last = true, required = true, value_name = "COMMAND")]
        argv: Vec<String>,
    },
    /// Keep a server or worker running in the helper, and stop it.
    ///
    /// A job starts in the helper's home like run, in a session and process
    /// group of its own, and keeps running after this command returns, for
    /// up to --timeout seconds. Atlas also ends it when the person stops
    /// agents and when the helper comes on screen. Stopping checks that it
    /// is still the same process, then ends its group and says what is left.
    /// A process the job starts that calls setsid leaves the job's group and
    /// session. Atlas never stops it. If Atlas did not see it before its
    /// parent exited, it may also be missing from reports.
    Job {
        #[command(subcommand)]
        command: JobCommand,
    },
    /// Copy files a task made in the helper into a private folder here.
    ///
    /// Each FILE is a regular file owned by the helper, in its home (a path
    /// relative to it) or in /tmp, that has not changed for two seconds.
    /// The reply gives each copy's path, size and SHA-256. The file in the
    /// helper is left as it is.
    Collect {
        /// Files in the helper, at most 16.
        #[arg(required = true, value_name = "FILE", num_args = 1..=16)]
        files: Vec<String>,
        /// The largest file accepted, such as 200M (64M unless given, at most 1G).
        #[arg(long, value_name = "SIZE", default_value = "64M", value_parser = file_size)]
        max_size: u64,
    },
    /// The first process of a background job, started by the helper daemon.
    #[command(hide = true)]
    JobLaunch {
        #[arg(long)]
        log: PathBuf,
        #[arg(long)]
        argv0: String,
        #[arg(last = true, required = true)]
        program: Vec<String>,
    },
    /// Restart the helper after something went wrong.
    ///
    /// Confirms a new hidden helper before clearing an action of unknown outcome.
    /// Defers while another session is active; --allow-disrupt permits interrupting it.
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
    /// Verifies the release's checksum, Developer ID signature and version, then moves the helper onto it. Defers while another session or a background job is active; --allow-disrupt permits interrupting it.
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
        #[command(flatten)]
        picks: AgentPicks,
    },
    /// Disconnect Atlas from coding agents.
    ///
    /// Use --claude --app together to disconnect both Claude parts.
    Remove {
        #[command(flatten)]
        picks: AgentPicks,
    },
}

/// The agents and Claude app a connect or disconnect names; none named
/// means every installed one.
#[derive(clap::Args)]
pub(super) struct AgentPicks {
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
    pub(super) app: bool,
}

impl AgentPicks {
    /// The coding agents picked.
    pub(super) fn agents(&self) -> Vec<atlas::agents::Agent> {
        use atlas::agents::Agent;

        [
            (self.claude, Agent::Claude),
            (self.codex, Agent::Codex),
            (self.opencode, Agent::Opencode),
            (self.pi, Agent::Pi),
            (self.cursor, Agent::Cursor),
        ]
        .into_iter()
        .filter_map(|(on, agent)| on.then_some(agent))
        .collect()
    }
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
    /// List copies into the helper and their state, newest first.
    ///
    /// Shows each plan's ID, state and the files it copies, never their
    /// contents, and which session made it. Plans rolled back completely
    /// are left out unless --all.
    List {
        /// Include plans rolled back completely.
        #[arg(long)]
        all: bool,
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

#[derive(Subcommand)]
pub(super) enum JobCommand {
    /// Start a job and print its id.
    Start {
        /// A short name to know it by; the program's name unless given.
        #[arg(long, value_parser = job_name)]
        name: Option<String>,
        /// Seconds before Atlas stops it, 1 to 86400.
        #[arg(long, value_name = "SECONDS", default_value_t = 3600, value_parser = job_seconds)]
        timeout: u64,
        /// The program and its arguments, after `--`.
        #[arg(last = true, required = true, value_name = "COMMAND")]
        argv: Vec<String>,
    },
    /// List this task's jobs; --all lists every job in the helper.
    List {
        #[arg(long)]
        all: bool,
    },
    /// Print the end of a job's output.
    Log {
        #[arg(value_parser = job_id)]
        id: JobId,
        /// Bytes from the end, at most 1048576.
        #[arg(long, default_value_t = 16384, value_parser = log_bytes)]
        bytes: usize,
        /// Read a job of another task, after that task's session was lost.
        #[arg(long, value_name = "KEY", value_parser = session_key)]
        task: Option<Key>,
    },
    /// Stop a job: TERM, then KILL after --grace seconds.
    Stop {
        #[arg(value_parser = job_id)]
        id: JobId,
        /// Seconds between TERM and KILL, 1 to 60.
        #[arg(long, default_value_t = 5, value_parser = grace_seconds)]
        grace: u64,
        /// Stop a job of another task, after that task's session was lost.
        #[arg(long, value_name = "KEY", value_parser = session_key)]
        task: Option<Key>,
    },
}

/// `job start --timeout`: whole seconds a job may run.
fn job_seconds(value: &str) -> std::result::Result<u64, String> {
    let most = atlas::daemon::protocol::JOB_LIFETIME_MAX.as_secs();
    value
        .parse()
        .ok()
        .filter(|seconds| (1..=most).contains(seconds))
        .ok_or_else(|| format!("Give 1 to {most} seconds."))
}

/// `job stop --grace`: whole seconds between TERM and KILL.
fn grace_seconds(value: &str) -> std::result::Result<u64, String> {
    let most = atlas::daemon::protocol::JOB_GRACE_MAX.as_secs();
    value
        .parse()
        .ok()
        .filter(|seconds| (1..=most).contains(seconds))
        .ok_or_else(|| format!("Give 1 to {most} seconds."))
}

/// `job log --bytes`: how much of the end of a job's output.
fn log_bytes(value: &str) -> std::result::Result<usize, String> {
    let most = atlas::daemon::protocol::JOB_LOG_MAX;
    value
        .parse()
        .ok()
        .filter(|bytes| (1..=most).contains(bytes))
        .ok_or_else(|| format!("Give 1 to {most} bytes."))
}

/// A job's ID, as `job start` printed it.
fn job_id(value: &str) -> std::result::Result<JobId, String> {
    JobId::parse(value).ok_or_else(|| "Give the job's id, as atlas job start printed it.".into())
}

/// `job start --name`: a short name to know a job by.
fn job_name(value: &str) -> std::result::Result<String, String> {
    atlas::daemon::protocol::job_name(value)
        .then(|| value.to_owned())
        .ok_or_else(|| {
            "Give a name of 1 to 64 characters, none of them a control character.".into()
        })
}

/// `collect --max-size`: a whole number of bytes with an optional K, M or
/// G (binary units), from 1 byte to 1 GiB.
fn file_size(value: &str) -> std::result::Result<u64, String> {
    let refused = || "Give a size such as 200M, at most 1G.".to_owned();
    let (digits, unit) = match value.char_indices().last() {
        Some((at, unit)) if unit.is_ascii_alphabetic() => (&value[..at], Some(unit)),
        _ => (value, None),
    };
    let scale: u64 = match unit.map(|unit| unit.to_ascii_uppercase()) {
        None => 1,
        Some('K') => 1 << 10,
        Some('M') => 1 << 20,
        Some('G') => 1 << 30,
        Some(_) => return Err(refused()),
    };
    digits
        .parse::<u64>()
        .ok()
        .and_then(|count| count.checked_mul(scale))
        .filter(|bytes| (1..=atlas::daemon::protocol::READ_SIZE_MAX).contains(bytes))
        .ok_or_else(refused)
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

/// `screenshot --region`: X,Y,W,H in display points.
fn region(value: &str) -> std::result::Result<Region, String> {
    let [x, y, width, height] = value
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<std::result::Result<Vec<f64>, _>>()
        .ok()
        .and_then(|numbers| <[f64; 4]>::try_from(numbers).ok())
        .filter(|numbers| numbers.iter().all(|number| number.is_finite()))
        .ok_or("Give the region as X,Y,W,H in display points, for example 100,80,800,600.")?;

    if width <= 0.0 || height <= 0.0 {
        return Err("The region's width and height must be more than 0.".into());
    }

    Region::from_points(x, y, width, height).ok_or_else(|| {
        "Give a region within 16384 points of the display's corner, at most 16384 points across."
            .into()
    })
}

/// `activity --session`: a session's key, as `activity --all` lists it.
fn session_key(value: &str) -> std::result::Result<Key, String> {
    Key::parse(value).ok_or_else(|| {
        "Give a session's key: 12 lowercase hex digits, as atlas activity --all lists them.".into()
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

/// `--wait` or `ATLAS_WAIT`: whole seconds, 0 for none.
fn control_wait(value: &str) -> std::result::Result<Duration, String> {
    value
        .parse()
        .ok()
        .map(Duration::from_secs)
        .filter(|wait| *wait <= CONTROL_WAIT_MAX)
        .ok_or_else(|| format!("Give 0 to {} seconds.", CONTROL_WAIT_MAX.as_secs()))
}

/// The wait a command that waits is given: `--wait`'s, else the variable
/// `ATLAS_WAIT`'s, else the default. Only such a command reads the
/// variable, so a bad one stops no other command.
pub(super) fn control_wait_given(
    flag: Option<Duration>,
    variable: Option<&OsStr>,
) -> std::result::Result<Duration, clap::Error> {
    match (flag, variable) {
        (Some(wait), _) => Ok(wait),
        (None, Some(variable)) => control_wait.parse_ref(
            &Cli::command(),
            Some(&clap::Arg::new("ATLAS_WAIT").value_name("ATLAS_WAIT")),
            variable,
        ),
        (None, None) => Ok(CONTROL_WAIT),
    }
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

/// The helper app a command is about.
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

    /// Where input must land: this app, and the `window` and `field` when
    /// named.
    pub(super) fn expected(
        self,
        window: Option<i64>,
        field: Option<String>,
    ) -> atlas::error::Result<Option<agent::Expected>> {
        agent::Expected::new(self.target(), window, field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowing_disruption_is_separate_from_reinstalling_an_older_release() {
        let cli = parse(&["update", "--force"]).unwrap();
        assert!(!cli.allow_disrupt);
        assert!(matches!(
            cli.command,
            Some(Command::Update { force: true, .. })
        ));

        let cli = parse(&["update", "--allow-disrupt"]).unwrap();
        assert!(cli.allow_disrupt);
        assert!(matches!(
            cli.command,
            Some(Command::Update { force: false, .. })
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
        assert_eq!(job_seconds("86400"), Ok(86400));
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
                    Some(Command::Activity {
                        all,
                        session,
                        limit,
                    }),
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
                    Some(Command::Cleanup {
                        apply,
                        session,
                        task,
                    }),
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
                        Command::Type { window, .. }
                        | Command::Key { window, .. }
                        | Command::Click { window, .. }
                        | Command::Scroll { window, .. }
                        | Command::Drag { window, .. },
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
                        Command::Type { target, .. }
                        | Command::Key { target, .. }
                        | Command::Scroll { target, .. }
                        | Command::Drag { target, .. }
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
            Ok(Cli { command: Some(Command::Key { keys, .. }), .. }) if keys == ["cmd+s"]
        ));
        assert!(parse(&["type", "hi", "--app", "A", "--pid", "1"]).is_err());
        assert!(parse(&["move", "1", "2", "--app", "A"]).is_err());
    }

    #[test]
    fn screenshots_take_an_app_or_a_region_never_both() {
        let given = |args: &[&str]| match parse(args) {
            Ok(Cli {
                command:
                    Some(Command::Screenshot {
                        target,
                        region,
                        max_dimension,
                        ..
                    }),
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
        use atlas::client::Size;

        assert_eq!(screenshot_size(None), Size::Standard);
        assert_eq!(screenshot_size(Some(0)), Size::Full);
        assert_eq!(screenshot_size(Some(800)), Size::Longest(800));
        assert_eq!(screenshot_size(Some(1456)), Size::Longest(1456));

        let given = |args: &[&str]| match parse(args) {
            Ok(Cli {
                command:
                    Some(Command::Screenshot {
                        max_dimension,
                        new_file,
                        ..
                    }),
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
            command: Some(Command::Type { text, stdin, .. }),
            ..
        }) = parse(&["type", "-hi there"])
        else {
            panic!("type did not parse");
        };

        assert_eq!((text.as_deref(), stdin), (Some("-hi there"), false));

        let Ok(Cli {
            command: Some(Command::Key { keys, .. }),
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
                    stdin: true,
                    ..
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
