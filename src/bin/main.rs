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
    /// Inspect the helper setup, or migrate capture to the private managed layout.
    #[command(
        long_about = "Without flags, report the helper account, configuration, endpoints and services without changing anything. --apply runs the unchanged capture daemon as a managed LaunchAgent on a private socket directory beside the existing job, moves input and the configuration over, then retires the old job, recording every step so an interruption resumes. --rollback restores the previous job and configuration."
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
    AgentGuide,
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
    let cli = Cli::parse();
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
        Command::AgentGuide => {
            print!("{}", agent::GUIDE);
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
        | Command::AgentGuide
        | Command::EnvironmentWorker { .. }
        | Command::SetupHelper { .. } => {
            unreachable!("helper-side commands return above")
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
        Command::Menubar { port, render } => {
            let executable = menubar::executable()?;
            match render {
                Some(directory) => viewer::render_menubar(client, &executable, &directory).await,
                None => {
                    viewer::serve(client, port, Some(executable)).await?;
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
            command: Command::Menubar { port, render },
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
