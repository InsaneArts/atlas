use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{io::Read, path::PathBuf, process::ExitCode};
use understudy::{
    client::Client,
    config::Config,
    environment,
    error::{Error, Result},
    session::diagnose,
    viewer,
};

#[derive(Parser)]
#[command(version, about = "Observe and control a prepared hidden macOS desktop")]
struct Cli {
    #[arg(
        long,
        env = "UNDERSTUDY_CONFIG",
        default_value = "understudy.local.json",
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
        long_about = "Preview an explicit one-way snapshot into an existing hidden login. The JSON manifest contains a files array: source (absolute or relative to the manifest), target (ASCII relative path inside the helper home), and optional executable. Stop applications that write these selected settings before applying. Existing untracked differences and helper-side edits are conflicts; managed updates keep backups. This does not create accounts, copy logins, or synchronize running application databases."
    )]
    Environment {
        /// JSON file listing only the files to provision.
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
    InputService {
        #[arg(long)]
        uid: u32,
        #[arg(long)]
        controller_uid: u32,
        #[arg(long)]
        socket: PathBuf,
    },
    /// Inspect the helper account, configuration, endpoints and services; changes nothing.
    Setup {
        /// Helper account short name. Defaults to the configured helper.
        #[arg(long)]
        helper: Option<String>,
    },
    /// Read session identity and permissions without prompting.
    Status,
    /// Report readiness and only the missing setup steps.
    Doctor,
    /// Atomically install this Rust executable at the configured shared path.
    Install,
    /// Start the Rust input service inside the prepared helper login.
    StartInput,
    /// Serve the local browser viewer with explicit mouse/keyboard control.
    Monitor {
        #[arg(long, default_value_t = 8765)]
        port: u16,
    },
    /// Save a real helper PNG.
    Capture {
        #[arg(long)]
        max_dimension: Option<u32>,
    },
    /// List applications in the helper session.
    Apps,
    /// Send a validated action array, or @path/to/actions.json.
    Input { actions: String },
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
async fn input_service(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<Value> {
    understudy::native::service::serve(uid, controller_uid, socket).await?;
    Ok(Value::Null)
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
        Command::SessionInfo => return understudy::native::session_info(),
        Command::InputService {
            uid,
            controller_uid,
            socket,
        } => return input_service(uid, controller_uid, socket).await,
        Command::Setup { helper } => {
            return understudy::setup::preview(&cli.config, helper.as_deref()).await;
        }
        command => command,
    };
    let client = Client::new(Config::load(&cli.config)?)?;
    match command {
        Command::InputService { .. }
        | Command::SessionInfo
        | Command::EnvironmentWorker { .. }
        | Command::Setup { .. } => {
            unreachable!("helper-side commands return above")
        }
        Command::Status => Ok(client.status().await),
        Command::Doctor => Ok(diagnose(client.status().await, client.config.helper_uid)),
        Command::Environment {
            manifest,
            apply,
            command,
        } => match command {
            Some(EnvironmentCommand::Profile { profile, apply }) => {
                environment::profile::sync(&client, &profile, apply).await
            }
            Some(EnvironmentCommand::Run { argv }) => environment::run(&client, argv).await,
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
            viewer::serve(client, port).await?;
            Ok(Value::Null)
        }
        Command::Capture { max_dimension } => client.capture(max_dimension).await,
        Command::Apps => client.apps().await,
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
        Command::Run { argv } => client.run(argv).await,
        Command::Restart => client.restart().await,
    }
}
