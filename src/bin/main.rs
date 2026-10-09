use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{io::Read, path::PathBuf, process::ExitCode};
use understudy::{
    client::Client,
    config::Config,
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = if matches!(cli.command, Command::SessionInfo) {
        understudy::native::session_info()
    } else {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(Error::from)
            .and_then(|runtime| runtime.block_on(execute(cli)))
    };
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
async fn execute(cli: Cli) -> Result<Value> {
    if matches!(cli.command, Command::SessionInfo) {
        return understudy::native::session_info();
    }
    #[cfg(target_os = "macos")]
    if let Command::InputService {
        uid,
        controller_uid,
        socket,
    } = cli.command
    {
        understudy::native::service::serve(uid, controller_uid, socket).await?;
        return Ok(Value::Null);
    }
    let client = Client::new(Config::load(&cli.config)?)?;
    match cli.command {
        Command::InputService { .. } | Command::SessionInfo => {
            unreachable!("handled before configuration")
        }
        Command::Status => Ok(client.status().await),
        Command::Doctor => Ok(diagnose(client.status().await, client.config.helper_uid)),
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
                {
                    let mut bytes = Vec::new();
                    std::fs::File::open(path)?
                        .take(32769)
                        .read_to_end(&mut bytes)?;
                    bytes
                }
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
