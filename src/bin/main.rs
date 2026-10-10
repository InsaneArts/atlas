use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{io::Read, path::PathBuf, process::ExitCode, time::Duration};
use understudy::{
    actions::Button,
    agent,
    client::Client,
    config::Config,
    environment,
    error::{Error, Result},
    menubar,
    transport::with_output,
    viewer,
};

#[derive(Parser)]
#[command(version, about = "Observe and control a prepared hidden macOS desktop")]
struct Cli {
    #[arg(
        long,
        env = "UNDERSTUDY_CONFIG",
        hide_env_values = true,
        default_value = understudy::config::DEFAULT_PATH,
        global = true
    )]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Discover and provision selected parts of the helper environment.
    #[command(
        args_conflicts_with_subcommands = true,
        long_about = "Preview an explicit one-way snapshot into an existing hidden login. The TOML manifest lists [[files]] tables: source (absolute or relative to the manifest), target (ASCII relative path inside the helper home), and optional executable. Stop applications that write these selected settings before applying. Existing untracked differences and helper-side edits are conflicts; managed updates keep backups. This does not create accounts, copy logins, or synchronize running application databases."
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
    /// Inspect the helper setup, or move it to Understudy's own helper daemon.
    #[command(
        long_about = "Without flags, report the helper account, configuration, endpoints and services without changing anything. --apply runs Understudy's own signed helper daemon as a managed LaunchAgent on a private socket directory beside the existing capture job, moves input and the configuration over once it is verified, then retires the old job, recording every step so an interruption resumes. --rollback restores the previous job and configuration."
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
    /// Print the resolved configuration (absolute paths) as JSON.
    Config,
    /// Read session identity and permissions without prompting.
    Status,
    /// Report readiness and only the missing setup steps.
    Doctor,
    /// Exercise real native Save/Open dialogs with the AppKit fixture and record the outcome.
    VerifyNative {
        /// Compiled tests/live/DesktopFixture.swift inside a helper-readable path.
        #[arg(long, default_value = understudy::capability::FIXTURE)]
        fixture: PathBuf,
    },
    /// Atomically install this Rust executable at the configured shared path.
    Install,
    /// Start the Rust input service inside the prepared helper login.
    StartInput,
    /// Serve the local browser viewer with explicit mouse/keyboard control.
    Monitor {
        #[arg(long, default_value_t = 8765)]
        port: u16,
    },
    /// Show the helper in the macOS menu bar, serving the viewer it opens.
    /// Quitting the menu bar stops both and releases control.
    Menubar {
        #[arg(long, default_value_t = 8765)]
        port: u16,
        /// Instead, draw the panel in light and dark to PNGs in DIR and exit.
        #[arg(long, value_name = "DIR")]
        render: Option<PathBuf>,
        /// Instead, turn "Open at Login" on or off, or report it.
        #[arg(long, value_name = "on|off|status", conflicts_with = "render")]
        login: Option<LoginArg>,
    },
    /// Save a real helper PNG; reports pointsPerPixel for input coordinates.
    #[command(visible_alias = "screenshot")]
    Capture {
        #[arg(long)]
        max_dimension: Option<u32>,
    },
    /// List applications in the helper session.
    Apps,
    /// Send a validated action array, or @path/to/actions.json.
    Input { actions: String },
    /// Click at display point X Y (screenshot pixels × pointsPerPixel), or on
    /// exactly one accessibility element of an app with --app and --name/--role.
    Click {
        #[arg(requires = "y")]
        x: Option<f64>,
        y: Option<f64>,
        #[command(flatten)]
        target: AppTarget,
        /// Exact element title, description or identifier.
        #[arg(long, conflicts_with = "x")]
        name: Option<String>,
        /// Accessibility role, for example AXButton.
        #[arg(long, conflicts_with = "x")]
        role: Option<String>,
        #[arg(long, value_enum, default_value_t)]
        button: ButtonArg,
        #[arg(long, default_value_t = 1)]
        count: u8,
        /// Comma-separated: cmd, ctrl, alt, shift, fn.
        #[arg(long, value_delimiter = ',')]
        modifiers: Vec<String>,
    },
    /// List accessibility elements of an app's windows (role, name, value, frame).
    Elements {
        #[command(flatten)]
        target: AppTarget,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value_t = 200)]
        max: usize,
    },
    /// Print a concise guide for an LLM agent using Understudy.
    AgentGuide {
        /// Print the agent plugin's skill (SKILL.md) instead.
        #[arg(long, hide = true, conflicts_with = "commands")]
        skill: bool,
        /// Print the command reference (Markdown) instead.
        #[arg(long, hide = true)]
        commands: bool,
    },
    /// Whether selected tools run and apps are installed in the helper account.
    Readiness {
        #[arg(long = "tool")]
        tools: Vec<String>,
        #[arg(long = "app")]
        apps: Vec<String>,
    },
    /// Move the pointer to display point X Y.
    Move { x: f64, y: f64 },
    /// Press at FROM, move to TO and release.
    Drag {
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        #[arg(long, value_enum, default_value_t)]
        button: ButtonArg,
    },
    /// Scroll at X Y by lines; negative values scroll up or left.
    Scroll {
        x: f64,
        y: f64,
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        dx: i32,
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        dy: i32,
    },
    /// Type text, split into batches without breaking characters.
    Type {
        #[arg(required_unless_present = "stdin")]
        text: Option<String>,
        /// Read the text from standard input instead.
        #[arg(long, conflicts_with = "text")]
        stdin: bool,
    },
    /// Press keys in order, for example `key cmd+s` or `key escape return`.
    Key {
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// List helper windows front to back (document windows unless --all).
    Windows {
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Open an app, files or URLs inside the helper.
    Open {
        #[arg(long)]
        app: Option<String>,
        targets: Vec<String>,
    },
    /// Wait until a matching window appears, or with --gone until it closes.
    Wait {
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        gone: bool,
        /// Seconds.
        #[arg(long, default_value_t = 10)]
        timeout: u64,
    },

    /// Execute argv in the helper session.
    Run {
        #[arg(last = true, required = true)]
        argv: Vec<String>,
    },
    /// Restart the helper and confirm a new hidden process before clearing uncertainty.
    Restart,
    /// Switch to another login, such as the helper. macOS shows that
    /// account's login window; its password is typed there, never here.
    Switch {
        /// Account short name. Without it, choose from a list.
        account: Option<String>,
        /// Show the login window without choosing an account.
        #[arg(long, conflicts_with = "account")]
        login_window: bool,
        /// Only list the accounts that can be switched to.
        #[arg(long)]
        list: bool,
        /// Switch without asking (it changes the person's screen).
        #[arg(long)]
        yes: bool,
    },
    /// Update Understudy.app to the latest release after verifying its
    /// checksum and Developer ID signature, then move the helper onto it.
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
    /// Set Understudy up on this Mac, step by step: helper account, first
    /// sign-in, permissions, the helper job and input. Safe to run again.
    Init,
    /// Remove Understudy from this Mac (asks first). The helper account and
    /// its files stay; the app goes to the Trash.
    Uninstall {
        /// Keep configuration, setup records and logs.
        #[arg(long)]
        keep_config: bool,
        /// Do not ask.
        #[arg(long)]
        yes: bool,
    },
    /// Connect Claude Code and Codex to Understudy (a plugin that teaches the
    /// agent to use it and guides setup), or show which are connected.
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
        #[arg(long)]
        claude: bool,
        #[arg(long)]
        codex: bool,
    },
    /// Disconnect the plugin from Claude Code and/or Codex.
    Remove {
        #[arg(long)]
        claude: bool,
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
        #[arg(long)]
        apply: bool,
    },
    /// Run a command with the selected helper environment and helper tool paths.
    Run {
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
    let mut cli = Cli::parse();
    cli.config = understudy::config::effective(cli.config);
    // The helper daemon keeps this thread for the Core Foundation run loop.
    #[cfg(target_os = "macos")]
    if let Command::HelperServe {
        uid,
        controller_uid,
        socket,
    } = cli.command
    {
        return match understudy::daemon::serve::serve_blocking(uid, controller_uid, socket) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{}", json!(error));
                ExitCode::FAILURE
            }
        };
    }
    let result = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(Error::from)
        .and_then(|runtime| runtime.block_on(execute(cli)));
    match result {
        Ok(result) => {
            if !result.is_null() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
                );
            }
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
            eprintln!("{}", json!(error));
            ExitCode::FAILURE
        }
    }
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

async fn execute(cli: Cli) -> Result<Value> {
    // Helper-side commands run inside the helper login and never read the
    // controller's configuration.
    let command = match cli.command {
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
        } => return update(&cli.config, allow_unnotarized, force, version),
        Command::UpdateFinish => return update_finish(&cli.config),
        Command::Init => return understudy::init::run(&cli.config).await,
        Command::Agents { action } => {
            return match action.unwrap_or(AgentsCommand::Status { brief: false }) {
                AgentsCommand::Status { brief: true } => {
                    println!("{}", understudy::agents::brief(&cli.config).await);
                    Ok(Value::Null)
                }
                AgentsCommand::Status { brief: false } => Ok(understudy::agents::status()),
                AgentsCommand::Install { claude, codex } => {
                    understudy::agents::change(&chosen(claude, codex), false)
                }
                AgentsCommand::Remove { claude, codex } => {
                    understudy::agents::change(&chosen(claude, codex), true)
                }
            };
        }
        Command::Uninstall { keep_config, yes } => {
            return understudy::uninstall::run(&cli.config, keep_config, yes).await;
        }
        Command::Menubar {
            login: Some(login), ..
        } => return menubar::login(login.as_str()),
        Command::Switch {
            account,
            login_window,
            list,
            yes,
        } => return switch(&cli.config, account, login_window, list, yes),
        Command::Setup {
            helper,
            apply: false,
            rollback: false,
        } => return understudy::setup::preview(&cli.config, helper.as_deref()).await,
        command => command,
    };
    let client = Client::new(Config::load(&cli.config)?)?;
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
                understudy::setup::migrate::apply(&cli.config, &ownership).await
            } else {
                understudy::setup::migrate::rollback(&cli.config, &ownership).await
            }
        }
        Command::Config => Ok(json!({ "ok": true, "config": client.config })),
        Command::Status => Ok(client.status().await),
        Command::Doctor => Ok(understudy::capability::doctor(
            client.status().await,
            client.config.helper_uid,
        )),
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
                        "Provide a manifest or use environment discover; see environment --help",
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
        Command::Menubar { port, render, .. } => {
            let program = menubar::Program::new(&cli.config)?;
            match render {
                Some(directory) => viewer::render_menubar(client, &program, &directory).await,
                None => {
                    viewer::serve(client, port, Some(program)).await?;
                    Ok(Value::Null)
                }
            }
        }
        Command::Capture { max_dimension } => client.capture(max_dimension).await,
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
        Command::Open { app, targets } => agent::open(&client, app.as_deref(), &targets).await,
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
        Command::Restart => client.restart().await,
    }
}

/// Ask the person on the terminal; never assume yes without one.
fn confirm(question: &str) -> Result<bool> {
    use std::io::{BufRead, IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        return Err(Error::new(
            "switch",
            "Switching changes the person's screen: run it in a terminal, or pass --yes",
        ));
    }
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// List, choose and switch to another login.
fn switch(
    config: &std::path::Path,
    account: Option<String>,
    login_window: bool,
    list: bool,
    yes: bool,
) -> Result<Value> {
    use std::io::{BufRead, IsTerminal, Write};
    use understudy::switch;
    let helper = Config::load(config).ok().map(|config| config.helper_uid);
    let accounts = switch::accounts(helper)?;
    if list {
        return Ok(json!({"ok": true, "accounts": accounts}));
    }
    if login_window {
        if !yes && !confirm("Show the login window now?")? {
            return Ok(json!({"ok": false, "cancelled": true}));
        }
        return switch::to_login_window();
    }
    let chosen = match account {
        Some(name) => accounts
            .iter()
            .find(|candidate| candidate.name == name)
            .cloned()
            .ok_or_else(|| Error::new("switch", format!("No login named {name} to switch to")))?,
        None => {
            if !std::io::stdin().is_terminal() {
                return Ok(json!({"ok": true, "accounts": accounts}));
            }
            for (index, candidate) in accounts.iter().enumerate() {
                let label = candidate.real_name.as_deref().unwrap_or(&candidate.name);
                let helper = if candidate.helper { "  (helper)" } else { "" };
                eprintln!("  {}. {label} — {}{helper}", index + 1, candidate.name);
            }
            eprint!("Switch to which login? ");
            std::io::stderr().flush()?;
            let mut answer = String::new();
            std::io::stdin().lock().read_line(&mut answer)?;
            let index = answer
                .trim()
                .parse::<usize>()
                .ok()
                .and_then(|number| number.checked_sub(1))
                .filter(|index| *index < accounts.len())
                .ok_or_else(|| Error::new("switch", "No such choice"))?;
            return switch::to(&accounts[index]);
        }
    };
    if !yes && !confirm(&format!("Switch to {} now?", chosen.name))? {
        return Ok(json!({"ok": false, "cancelled": true}));
    }
    switch::to(&chosen)
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
    let mut report = understudy::update::install(&source, allow_unnotarized, force)?;
    drop(exclusion);
    if report["updated"] != true {
        return Ok(report);
    }
    let executable = understudy::update::installed_app()?.join("Contents/MacOS/understudy");
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
         JSON and exits non-zero on failure.\n",
    );
    let mut root = Cli::command().bin_name("understudy");
    root.build();
    for command in root.get_subcommands_mut() {
        if command.is_hide_set() {
            continue;
        }
        let name = command.get_name().to_owned();
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
            command: Command::Menubar { port, render, .. },
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
            command: Command::Scroll { dy, .. },
            ..
        }) = parse(&["scroll", "1", "2", "--dy", "-3"])
        else {
            panic!("scroll did not parse");
        };
        assert_eq!(dy, -3);
    }
}
