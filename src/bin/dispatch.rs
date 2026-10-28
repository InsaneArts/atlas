//! Command execution, activity attribution and helper-change policy.
use super::{
    cli::{
        AgentsCommand, ButtonArg, Command, CursorHookEvent, EnvironmentCommand, OnOff,
        OnboardingCommand, WindowState,
    },
    doctor, help,
    output::print,
    update,
};
use atlas::{
    agent,
    client::{Actor, Client, Size},
    config::Config,
    doing::Doing,
    environment,
    error::{Error, Result},
    host_session::{self, HostSession, store::Kind},
    menubar,
    transport::with_output,
    ui::{self, progress::Spinner},
    viewer,
};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

/// How large a screenshot is saved: at most one pixel per display point and
/// [`atlas::client::SCREENSHOT_SIDE`] pixels on its longer side unless a
/// longest side is given; 0 asks for the full size.
pub(super) fn screenshot_size(given: Option<u32>) -> Size {
    match given {
        None => Size::Standard,
        Some(0) => Size::Full,
        Some(side) => Size::Longest(side),
    }
}

/// Connect or disconnect the named agents and Claude app; none
/// named means every installed one, the Claude app included.
fn changed(
    claude: bool,
    codex: bool,
    opencode: bool,
    pi: bool,
    cursor: bool,
    app: bool,
    remove: bool,
) -> Result<Value> {
    // Connecting an app that isn't here would only leave its settings behind.
    if app && !remove && !atlas::claude_app::installed() {
        return Err(Error::new("agents", "The Claude app isn't installed"));
    }
    if app {
        atlas::agents::change_these(
            &chosen(claude, codex, opencode, pi, cursor),
            false,
            true,
            remove,
        )
    } else {
        atlas::agents::change(&chosen(claude, codex, opencode, pi, cursor), remove)
    }
}

/// The coding agents selected by flags (none means every installed one).
fn chosen(
    claude: bool,
    codex: bool,
    opencode: bool,
    pi: bool,
    cursor: bool,
) -> Vec<atlas::agents::Agent> {
    use atlas::agents::Agent;

    [
        (claude, Agent::Claude),
        (codex, Agent::Codex),
        (opencode, Agent::Opencode),
        (pi, Agent::Pi),
        (cursor, Agent::Cursor),
    ]
    .into_iter()
    .filter_map(|(on, agent)| on.then_some(agent))
    .collect()
}

/// What a command does in the helper, as the activity log keeps it: its
/// verb and the app and control it names. None for commands that act on
/// nothing there.
fn doing(command: &Command) -> Option<Doing> {
    Some(match command {
        Command::Screenshot { .. } => Doing::new("screenshot"),
        Command::Click {
            target,
            name,
            button,
            count,
            ..
        } => Doing::click(matches!(button, ButtonArg::Right), *count)
            .app(target.app.as_deref())
            .label(name.as_deref()),
        Command::Move { .. } => Doing::new("move"),
        Command::Drag { target, .. } => Doing::new("drag").app(target.app.as_deref()),
        Command::Scroll { target, .. } => Doing::new("scroll").app(target.app.as_deref()),
        Command::Type { target, .. } => Doing::new("type").app(target.app.as_deref()),
        Command::Key { keys, target } => Doing::keys(keys).app(target.app.as_deref()),
        Command::Open { app, targets, .. } => Doing::open(app.as_deref(), targets),
        Command::Close { app, .. } => Doing::new("close").app(app.as_deref()),
        Command::Run { .. }
        | Command::Environment {
            command: Some(EnvironmentCommand::Run { .. }),
            ..
        } => Doing::new("run"),
        _ => return None,
    })
}

/// Whether a command is its host session's work: everything an agent runs
/// but the servers and helper-side commands, which serve many sessions or
/// none, and the person's own setup.
fn attributed(command: &Command) -> bool {
    !matches!(
        command,
        Command::Mcp { .. }
            | Command::Watch { .. }
            | Command::Menubar { .. }
            | Command::Monitor { .. }
            | Command::HelperServe { .. }
            | Command::InputService { .. }
            | Command::EnvironmentWorker { .. }
            | Command::SessionInfo
            | Command::PermissionsInfo
            | Command::WindowsInfo
            | Command::ElementsInfo { .. }
            | Command::Init
            | Command::Uninstall { .. }
            | Command::Update { .. }
            | Command::UpdateFinish
            | Command::Setup { .. }
            | Command::SetupHelper { .. }
            | Command::Switch { .. }
            | Command::Onboarding { .. }
    )
}

/// Whether `command`, run by `actor`, is labelled with the host session it
/// comes from: an agent's own work only. What the person does from the app,
/// the menu bar or a viewer never is.
fn labelled(command: &Command, actor: Actor) -> bool {
    actor == Actor::Agent && attributed(command)
}

/// What a command is, as far as its host session's record cares: nothing
/// of its arguments.
fn kind(command: &Command) -> Kind {
    match command {
        Command::Screenshot { .. } => Kind::Screenshot,
        Command::Open { .. } => Kind::Open,
        Command::Close { .. } => Kind::Close,
        Command::Environment {
            apply,
            command: None,
            ..
        } => Kind::Environment { changes: *apply },
        Command::Environment {
            command: Some(EnvironmentCommand::Apply { .. }),
            ..
        } => Kind::Environment { changes: true },
        Command::Environment {
            command:
                Some(
                    EnvironmentCommand::Rollback { apply, .. }
                    | EnvironmentCommand::Profile { apply, .. },
                ),
            ..
        } => Kind::Environment { changes: *apply },
        _ => Kind::Other,
    }
}

/// Whether a command changes the helper, so an agent's is refused while
/// the person has stopped agents. Looking, reading and recovery are not.
fn changes_helper(command: &Command, attributed: bool) -> bool {
    match command {
        Command::Screenshot { .. } => false,
        // Each changes what runs in the helper, or how; a stop that moved
        // with a migrated layout would be lost.
        Command::Input { .. }
        | Command::Logout { .. }
        | Command::VerifyNative { .. }
        | Command::Install
        | Command::Update { check: false, .. } => true,
        Command::Setup {
            apply, rollback, ..
        } => *apply || *rollback,
        Command::Environment { apply, command, .. } => match command {
            None => *apply,
            Some(EnvironmentCommand::Run { .. } | EnvironmentCommand::Apply { .. }) => true,
            Some(EnvironmentCommand::Rollback { apply, .. }) => *apply,
            Some(EnvironmentCommand::Profile { apply, .. }) => *apply,
            Some(_) => false,
        },
        _ => attributed,
    }
}

/// Whether an agent's command may take control of the helper: it waits for
/// another command to finish first. A logout that asks first is the
/// person's at a terminal, and answers at once. Whoever runs one, a signal
/// stops it cleanly: nothing more starts, and the step under way settles
/// or is cut. `logout --yes` run at a terminal is the exception: it keeps
/// the stop every logout at a terminal has, which ends it at once.
pub(super) fn controls(command: &Command) -> bool {
    matches!(
        command,
        Command::Input { .. }
            | Command::Click { .. }
            | Command::Move { .. }
            | Command::Drag { .. }
            | Command::Scroll { .. }
            | Command::Type { .. }
            | Command::Key { .. }
            | Command::Open { .. }
            | Command::Close { .. }
            | Command::Run { .. }
            | Command::Restart
            | Command::StartInput
            | Command::Install
            | Command::Readiness { .. }
            | Command::Environment { .. }
            | Command::VerifyNative { .. }
            | Command::Logout { yes: true, .. }
    )
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
        "reasons": snapshot.stops,
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

/// Who runs this command: what the person does from the app is recorded as
/// theirs, not an agent's.
pub(super) fn actor() -> Actor {
    if std::env::var_os("ATLAS_ACTOR").is_some_and(|actor| actor == "viewer") {
        Actor::Viewer
    } else {
        Actor::Agent
    }
}

/// Whether `command`, run by `actor`, waits for another controller to let
/// go of the helper: only an agent's command that takes control does. What
/// the person does from the app answers at once, and so do servers, which
/// take no control of their own: a wait blocks the thread it runs on.
pub(super) fn waits(command: &Command, actor: Actor) -> bool {
    controls(command) && actor == Actor::Agent
}

/// Carry out `command`. When it [`waits`], it waits for up to `wait` in all
/// for other controllers to let go of the helper. An agent's command is
/// labelled with the host session it comes from, and what it left is noted
/// in that session's record; a refusal names the other sessions it can.
pub(super) async fn execute(config: PathBuf, command: Command, wait: Duration) -> Result<Value> {
    let actor = actor();

    // Helper-side commands run inside the helper login and never read the
    // controller's configuration. A command that talks to the helper loads
    // it before anything else.
    let doing = doing(&command);
    let kind = kind(&command);
    let changes = changes_helper(&command, doing.is_some());
    let waits = waits(&command, actor);
    let host = labelled(&command, actor)
        .then(HostSession::from_env)
        .flatten();
    let client = || {
        Client::new(Config::load(&config)?).map(|mut client| {
            client.actor = actor;
            client.doing = doing.clone();
            client.host_session = host.clone();
            client.controller = client.controller.labelled(host.as_ref());
            if waits {
                client.controller = client.controller.waiting(wait);
            }
            // What a Stop refuses at its start, it also refuses while the
            // command waits.
            if waits && changes {
                client.controller = client.controller.heeding_stop();
            }
            client
        })
    };

    // The person's stop refuses an agent's change at its start; one already
    // under way is the input service's to cut off.
    if changes {
        client()?.require_going().await?;
    }

    let result = carry_out(&config, command, &client, host.as_ref()).await;

    // What the command left for its session to find again, once it ended,
    // and a plan it rolled back forgotten wherever it is listed: labels, so
    // a record that cannot be written changes nothing.
    if kind.recorded(doing.as_ref())
        && let Ok(client) = client()
    {
        if let Some(host) = &host {
            let _ = host_session::store::note(&client, host, kind, doing.as_ref(), &result);
        }
        let _ = host_session::store::forget_rolled_back(&client, kind, &result);
    }

    // A refusal that can name the other session, or say whose an unknown
    // outcome is, says so.
    host_session::explain::reply(result, || client().ok(), host.as_ref())
}

/// [`execute`]'s work once the command is labelled: `client` gives its
/// client, and `host` is its host session, if any.
async fn carry_out(
    config: &Path,
    command: Command,
    client: &impl Fn() -> Result<Client>,
    host: Option<&HostSession>,
) -> Result<Value> {
    match command {
        Command::EnvironmentWorker { packet, apply } => environment::worker(&packet, apply),
        Command::AgentGuide {
            skill,
            commands,
            errors,
        } => {
            if skill {
                print(&agent::skill());
            } else if commands {
                print(&help::command_reference());
            } else if errors {
                print(agent::ERRORS);
            } else {
                print(agent::GUIDE);
            }
            Ok(Value::Null)
        }
        Command::SessionInfo => atlas::native::session_info(),
        Command::PermissionsInfo => Ok(atlas::daemon::facts::permissions_now()),
        Command::WindowsInfo => windows_info(),
        Command::ElementsInfo { pid } => elements_info(pid),
        Command::HelperServe { .. } => {
            unreachable!("main serves the helper daemon on its own thread")
        }
        Command::InputService {
            uid,
            controller_uid,
            socket,
        } => input_service(uid, controller_uid, socket).await,
        Command::SetupHelper {
            request,
            account,
            logged_out,
        } => atlas::setup::helper::run_guarded(
            &request,
            &atlas::setup::helper::Guard {
                account: Some(serde_json::from_str(&account)?),
                logged_out,
            },
        ),
        Command::Update {
            check: true,
            version,
            ..
        } => atlas::update::newer(&atlas::update::Source::from_env(version)),
        Command::Update {
            allow_unnotarized,
            force,
            version,
            check: false,
        } => {
            as_yourself("update")?;
            update::update(config, allow_unnotarized, force, version).await
        }
        Command::UpdateFinish => {
            as_yourself("update")?;
            update::update_finish(config).await
        }
        Command::Init => {
            as_yourself("init")?;
            atlas::init::run(config).await
        }
        Command::Onboarding { step } => {
            use atlas::onboarding;

            match step {
                OnboardingCommand::Status => onboarding::status(config).await,
                OnboardingCommand::Choose { account } => onboarding::choose(config, &account),
                OnboardingCommand::Create { account, full_name } => {
                    onboarding::create(config, &account, &full_name)
                }
                OnboardingCommand::Accept {
                    controller_uid,
                    controller_name,
                } => onboarding::accept(controller_uid, &controller_name),
                OnboardingCommand::AcceptWorker { ticket } => {
                    onboarding::accept_worker(&ticket).await
                }
                OnboardingCommand::Window { state } => {
                    onboarding::window(state == WindowState::Open)
                }
                OnboardingCommand::Forget => onboarding::forget(config),
                OnboardingCommand::Close { helper: false } => onboarding::close_leftovers(config),
                OnboardingCommand::Close { helper: true } => onboarding::close_helper(config),
                OnboardingCommand::Finish => onboarding::finish(config).await,
                OnboardingCommand::Login { state } => onboarding::login(config, state == OnOff::On),
            }
        }
        Command::Agents { action } => {
            match action.unwrap_or(AgentsCommand::Status { brief: false }) {
                AgentsCommand::Status { brief: true } => {
                    print(&format!("{}\n", atlas::agents::brief(config, host).await));
                    Ok(Value::Null)
                }
                AgentsCommand::Status { brief: false } => {
                    let _spinner = Spinner::new("Checking agent connections…");
                    Ok(atlas::agents::status())
                }
                AgentsCommand::CursorHook { event } => {
                    let event = match event {
                        CursorHookEvent::SessionStart => "session-start",
                        CursorHookEvent::BeforeShell => "before-shell",
                    };
                    let reply = atlas::agents::hook(config, event, std::io::stdin().lock()).await;
                    print(&format!("{reply}\n"));
                    Ok(Value::Null)
                }
                AgentsCommand::Project { dir } => {
                    atlas::agents::project(&dir.unwrap_or_else(|| PathBuf::from(".")))
                }
                AgentsCommand::Forget { dir } => {
                    atlas::agents::forget(&dir.unwrap_or_else(|| PathBuf::from(".")))
                }
                AgentsCommand::Install {
                    claude,
                    codex,
                    opencode,
                    pi,
                    cursor,
                    app,
                } => {
                    let _spinner = Spinner::new("Connecting…");
                    changed(claude, codex, opencode, pi, cursor, app, false)
                }
                AgentsCommand::Remove {
                    claude,
                    codex,
                    opencode,
                    pi,
                    cursor,
                    app,
                } => {
                    let _spinner = Spinner::new("Disconnecting…");
                    changed(claude, codex, opencode, pi, cursor, app, true)
                }
            }
        }
        Command::Uninstall {
            keep_config,
            yes,
            remove_helper,
        } => {
            as_yourself("uninstall")?;
            atlas::uninstall::run(config, keep_config, yes, remove_helper).await
        }
        Command::Menubar {
            login: Some(login), ..
        } => menubar::login(login.as_str()),
        Command::Switch {
            account,
            lock,
            list,
            yes,
        } => switch(config, account, lock, list, yes),
        Command::Setup {
            helper,
            apply: false,
            rollback: false,
        } => atlas::setup::preview(config, helper.as_deref()).await,
        Command::Setup { apply, .. } => {
            let client = client()?;
            // Recovery ownership: the migration settles only the pending
            // mutations its own log records and refuses any other.
            let ownership = client.controller.acquire(true)?;

            if apply {
                atlas::setup::migrate::apply(config, &ownership).await
            } else {
                atlas::setup::migrate::rollback(config, &ownership).await
            }
        }
        Command::Config => {
            let client = client()?;
            Ok(json!({ "ok": true, "config": client.config, "path": config }))
        }
        Command::Status => Ok(client()?.status().await),
        Command::Activity {
            all,
            session,
            limit,
        } => host_session::activity::list(&client()?, host, all, session.as_ref(), limit.into()),
        Command::Doctor {
            login_timing: true, ..
        } => atlas::login_log::timing(),
        Command::Doctor { verbose, .. } => Ok(doctor(&client()?, verbose).await),
        Command::VerifyNative { fixture } => atlas::capability::verify(&client()?, &fixture).await,
        Command::Environment {
            manifest,
            apply,
            command,
        } => {
            let client = client()?;

            match command {
                Some(EnvironmentCommand::Profile { profile, apply }) => {
                    environment::profile::sync(&client, &profile, apply).await
                }
                Some(EnvironmentCommand::Run { argv, timeout }) => {
                    environment::run(&client, argv, Duration::from_secs(timeout))
                        .await
                        .map(with_output)
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
                    environment::discovery::discover(
                        &client,
                        &tools,
                        &apps,
                        &environment,
                        &references,
                    )
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
            }
        }
        Command::Install => client()?.install(),
        Command::StartInput => client()?.start_input().await,
        Command::Watch { port, frames, once } => {
            atlas::watch::watch(client()?, port, frames, once).await?;
            Ok(Value::Null)
        }
        Command::Stop => client()?.hold(true).await,
        Command::Resume => client()?.hold(false).await,
        Command::Mcp {
            port,
            read_only,
            guard_app_controls,
        } => {
            atlas::mcp::serve(
                client()?,
                config.to_path_buf(),
                port,
                read_only,
                guard_app_controls,
            )
            .await?;
            Ok(Value::Null)
        }
        Command::Monitor { port } => {
            let client = client()?;
            viewer::monitor(config, port, client.output_dir()?).await?;
            Ok(Value::Null)
        }
        Command::Menubar {
            port,
            render,
            serve,
            ..
        } => {
            let client = client()?;
            let program = menubar::Program::new(config)?;

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
                    menubar::start(config, port, &output).await?;
                    Ok(Value::Null)
                }
            }
        }
        Command::Screenshot {
            target,
            region,
            max_dimension,
            new_file,
        } => {
            let target = target.target();
            let size = screenshot_size(max_dimension);
            agent::screenshot(&client()?, target.as_ref(), region, size, new_file).await
        }
        Command::Apps { all } => client()?.apps().await.map(|reply| listed_apps(reply, all)),
        Command::Click {
            x,
            y,
            target,
            name,
            role,
            button,
            count,
            modifiers,
        } => {
            let client = client()?;

            match (x, y, target.target()) {
                (Some(x), Some(y), target) => {
                    agent::act(
                        &client,
                        target.as_ref(),
                        &agent::click(x, y, button.into(), count, &modifiers),
                    )
                    .await
                }
                (None, None, Some(target)) => {
                    let filter = agent::ElementFilter { role, name };
                    agent::click_element(
                        &client,
                        &target,
                        &filter,
                        button.into(),
                        count,
                        &modifiers,
                    )
                    .await
                }
                _ => Err(Error::new(
                    "usage",
                    "Give X Y, or --app/--pid with --name/--role",
                )),
            }
        }
        Command::Elements {
            target,
            role,
            name,
            max,
            full,
        } => {
            let client = client()?;
            let target = target
                .target()
                .ok_or_else(|| Error::new("usage", "Select the app with --app or --pid"))?;
            let filter = agent::ElementFilter { role, name };
            agent::elements(&client, &target, &filter, max, full).await
        }
        Command::Readiness { tools, apps } => {
            atlas::readiness::readiness(&client()?, &tools, &apps).await
        }
        Command::Move { x, y } => {
            agent::act(
                &client()?,
                None,
                &json!([{ "type": "move", "x": x, "y": y }]),
            )
            .await
        }
        Command::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
            target,
        } => {
            agent::act(
                &client()?,
                target.target().as_ref(),
                &agent::drag((from_x, from_y), (to_x, to_y), button.into()),
            )
            .await
        }
        Command::Scroll {
            x,
            y,
            dx,
            dy,
            target,
        } => {
            if dx == 0 && dy == 0 {
                return Err(Error::new(
                    "usage",
                    "Give lines to scroll with --dy (down) or --dx (right), negative for up or left",
                ));
            }

            agent::act(
                &client()?,
                target.target().as_ref(),
                &json!([{ "type": "scroll", "x": x, "y": y, "dx": dx, "dy": dy }]),
            )
            .await
        }
        Command::Type {
            text,
            stdin,
            target,
        } => {
            let client = client()?;

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

            agent::type_text(&client, &text, target.target().as_ref()).await
        }
        Command::Key { keys, target } => {
            agent::act(&client()?, target.target().as_ref(), &agent::keys(&keys)?).await
        }
        Command::Windows { app, title, all } => {
            agent::list(&client()?, &agent::WindowFilter { app, title, all }).await
        }
        Command::Open {
            app,
            targets,
            environment,
        } => agent::open(&client()?, app.as_deref(), &targets, &environment).await,
        Command::Close { app, .. } => {
            let client = client()?;
            let closing = app
                .as_deref()
                .map_or(agent::Closing::All, agent::Closing::App);
            agent::close(&client, closing).await
        }
        Command::Wait {
            app,
            title,
            gone,
            timeout,
        } => {
            let client = client()?;
            let filter = agent::WindowFilter {
                app,
                title,
                all: false,
            };
            agent::wait(&client, &filter, gone, Duration::from_secs(timeout)).await
        }
        Command::Input { actions, target } => {
            let client = client()?;
            // The most an actions batch may take, in bytes.
            const LIMIT: u64 = 32 * 1024;

            let bytes = if let Some(path) = actions.strip_prefix('@') {
                let mut bytes = Vec::new();
                std::fs::File::open(path)
                    .map_err(|error| Error::new("io", format!("Could not read {path}: {error}")))?
                    .take(LIMIT + 1)
                    .read_to_end(&mut bytes)?;
                bytes
            } else {
                actions.into_bytes()
            };

            if bytes.len() as u64 > LIMIT {
                return Err(Error::new(
                    "input",
                    "Input exceeds 32 KiB; use short observed batches",
                ));
            }

            agent::act(
                &client,
                target.target().as_ref(),
                &serde_json::from_slice(&bytes)?,
            )
            .await
        }
        Command::Run { argv, timeout } => client()?
            .run_within(argv, Duration::from_secs(timeout))
            .await
            .map(with_output),
        Command::Restart => {
            let client = client()?;
            let _spinner = Spinner::new("Restarting the helper…");
            client.restart().await
        }
        Command::Logout { force, yes } => {
            let client = client()?;

            if !yes
                && !ui::prompt::confirm(
                    "Log out the helper? Its apps quit, and agents can't use it until you log in \
                     to it again.",
                    false,
                    "pass --yes",
                )?
            {
                return Err(ui::prompt::cancelled());
            }

            let _spinner = Spinner::new("Logging out the helper…");
            client.logout(force).await
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

/// Refuse `atlas <command>` run as root (with sudo): it would act for root's
/// home, not the person's, and leave files only root can change.
fn as_yourself(command: &str) -> Result<()> {
    refuse_root(command, nix::unistd::geteuid().is_root())
}

fn refuse_root(command: &str, root: bool) -> Result<()> {
    if root {
        return Err(Error::new(
            "usage",
            format!("Run atlas {command} as yourself, not with sudo."),
        ));
    }

    Ok(())
}

/// The helper's `apps` reply as `atlas apps` shows it: apps with a Dock
/// icon and menu bar apps that are not part of macOS, unless `all`, with
/// `more` saying how many processes were left out.
fn listed_apps(mut reply: Value, all: bool) -> Value {
    let Some(apps) = reply.get_mut("apps").and_then(Value::as_array_mut) else {
        return reply;
    };
    let before = apps.len();

    if !all {
        apps.retain(|app| {
            let executable = app["executable"].as_str().unwrap_or("");
            match app["policy"].as_str() {
                Some("regular") => true,
                Some("accessory") => {
                    !executable.is_empty()
                        && !executable.starts_with("/System/")
                        && !executable.starts_with("/usr/")
                }
                _ => false,
            }
        });
    }

    let more = before - apps.len();
    reply["more"] = json!(more);
    reply
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    fn parse(args: &[&str]) -> std::result::Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("atlas").chain(args.iter().copied()))
    }

    #[test]
    fn pi_and_opencode_can_be_selected_independently_or_together() {
        use atlas::agents::Agent;
        assert_eq!(chosen(false, false, false, true, false), [Agent::Pi]);
        assert_eq!(chosen(false, false, true, false, false), [Agent::Opencode]);
        assert_eq!(
            chosen(false, false, true, true, false),
            [Agent::Opencode, Agent::Pi]
        );
        for verb in ["install", "remove"] {
            assert!(parse(&["agents", verb, "--pi", "--opencode"]).is_ok());
        }
    }

    #[test]
    fn cursor_has_its_own_selector_and_internal_hooks_stay_out_of_help() {
        assert_eq!(
            chosen(false, false, false, false, true),
            [atlas::agents::Agent::Cursor]
        );
        for verb in ["install", "remove"] {
            assert!(parse(&["agents", verb, "--cursor", "--codex"]).is_ok());
        }
        assert!(parse(&["agents", "cursor-hook", "session-start"]).is_ok());
        let help = super::help::command()
            .find_subcommand_mut("agents")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(!help.contains("cursor-hook"));
    }

    #[test]
    fn apps_lists_apps_and_third_party_menu_bar_apps_unless_all() {
        let reply = json!({"ok": true, "apps": [
            {"name": "TextEdit", "pid": 1, "policy": "regular",
             "executable": "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit"},
            {"name": "Opie", "pid": 2, "policy": "accessory",
             "executable": "/Applications/Opie.app/Contents/MacOS/Opie"},
            {"name": "Siri", "pid": 3, "policy": "accessory",
             "executable": "/System/Library/CoreServices/Siri.app/Contents/MacOS/Siri"},
            {"name": "loginwindow", "pid": 4, "policy": "accessory",
             "executable": "/usr/libexec/loginwindow"},
            {"name": "cfprefsd", "pid": 5, "policy": "prohibited",
             "executable": "/usr/sbin/cfprefsd"},
        ]});

        let listed = listed_apps(reply.clone(), false);
        let names: Vec<&str> = listed["apps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|app| app["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["TextEdit", "Opie"]);
        assert_eq!(listed["more"], 3);
        assert_eq!(listed["ok"], true);

        let every = listed_apps(reply, true);
        assert_eq!(every["apps"].as_array().unwrap().len(), 5);
        assert_eq!(every["more"], 0);
    }

    #[test]
    fn a_process_that_cannot_be_placed_is_left_out_unless_all() {
        let reply = json!({"ok": true, "apps": [
            {"name": "Unnamed agent", "pid": 6, "policy": "accessory"},
            {"name": "No policy", "pid": 7,
             "executable": "/Applications/Tool.app/Contents/MacOS/Tool"},
            {"name": "Odd policy", "pid": 8, "policy": "unknown",
             "executable": "/Applications/Odd.app/Contents/MacOS/Odd"},
        ]});

        let listed = listed_apps(reply.clone(), false);
        assert_eq!(listed["apps"], json!([]));
        assert_eq!(listed["more"], 3);

        assert_eq!(
            listed_apps(reply, true)["apps"].as_array().unwrap().len(),
            3
        );
        assert_eq!(
            listed_apps(json!({"ok": false, "code": "apps"}), false),
            json!({"ok": false, "code": "apps"}),
            "a failed reply is passed on as it is"
        );
    }

    #[test]
    fn apps_takes_all_to_list_every_process() {
        assert!(matches!(
            parse(&["apps", "--all"]).unwrap().command,
            Some(Command::Apps { all: true })
        ));
        assert!(matches!(
            parse(&["apps"]).unwrap().command,
            Some(Command::Apps { all: false })
        ));
    }

    #[test]
    fn each_command_that_acts_in_the_helper_says_what_it_does() {
        let said = |args: &[&str]| {
            let command = parse(args).unwrap().command.unwrap();
            doing(&command).map(|doing| {
                let mut row = serde_json::Map::new();
                row.insert("ok".into(), json!(true));
                row.insert("actionCount".into(), json!(1));
                doing.fields(&mut row, true);
                atlas::doing::says(&Value::Object(row))
            })
        };
        let says = |args: &[&str], words: &str| assert_eq!(said(args).as_deref(), Some(words));

        says(&["screenshot"], "took a screenshot");
        says(
            &["click", "--app", "TextEdit", "--name", "Save"],
            "clicked Save in TextEdit",
        );
        says(&["click", "10", "20", "--count", "2"], "double-clicked");
        says(&["click", "10", "20", "--button", "right"], "right-clicked");
        says(&["move", "1", "2"], "moved the pointer");
        says(&["drag", "1", "2", "3", "4"], "dragged");
        says(&["scroll", "1", "2", "--dy", "3"], "scrolled");
        says(&["type", "secret"], "typed text");
        says(&["key", "cmd+s"], "pressed ⌘S");
        says(
            &["open", "--app", "TextEdit", "notes.txt"],
            "opened TextEdit",
        );
        says(&["open", "https://example.com"], "opened a link");
        says(&["close", "--app", "TextEdit"], "closed TextEdit");
        says(&["run", "--", "ls", "/private"], "ran a command");
        says(&["environment", "run", "--", "ls"], "ran a command");
        assert_eq!(
            said(&["windows"]),
            None,
            "a read does nothing in the helper"
        );
        assert_eq!(
            said(&["restart"]),
            None,
            "recovery is worded by its operation"
        );
    }

    #[test]
    fn an_actions_app_is_named_in_its_activity() {
        let app = |args: &[&str]| {
            let command = parse(args).unwrap().command.unwrap();
            let mut row = serde_json::Map::new();
            doing(&command).unwrap().fields(&mut row, true);
            row.get("app").cloned()
        };

        for args in [
            &["type", "--app", "TextEdit", "hi"][..],
            &["key", "--app", "TextEdit", "cmd+s"],
            &["scroll", "1", "2", "--dy", "3", "--app", "TextEdit"],
            &["drag", "1", "2", "3", "4", "--app", "TextEdit"],
            &["click", "1", "2", "--app", "TextEdit"],
        ] {
            assert_eq!(app(args), Some(json!("TextEdit")), "{args:?}");
        }
        assert_eq!(
            app(&["type", "--pid", "42", "hi"]),
            None,
            "a pid names no app"
        );
    }

    #[tokio::test]
    async fn a_stop_cancels_an_environment_run_after_dispatch() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use tokio::{
            io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
            net::UnixListener,
        };

        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let mut client = Client::new(Config {
            helper_uid: 60001,
            socket_path: dir.path().join("helper.sock"),
            identity_probe: dir.path().join("probe"),
            input_socket: dir.path().join("input.sock"),
            output_dir: dir.path().join("out"),
        })
        .unwrap();
        client.controller = atlas::control::Controller::at(60001, &dir.path().join("state"));
        client.transport.peer_uid = nix::unistd::getuid().as_raw();
        let command = parse(&["environment", "run", "--", "tool"])
            .unwrap()
            .command
            .unwrap();
        client.doing = doing(&command);
        let Command::Environment {
            command: Some(EnvironmentCommand::Run { argv, timeout }),
            ..
        } = command
        else {
            panic!("environment run did not parse");
        };
        let listener = UnixListener::bind(&client.transport.path).unwrap();
        let ownership = client.controller.acquire(false).unwrap();
        let helper = async {
            let proof = json!({"ok": true, "uid": 60001, "hasGraphicAccess": true,
                "parentPID": 123, "onConsole": false, "securitySessionID": 42,
                "displayID": 7, "width": 100, "height": 100});
            for (operation, reply) in [
                (
                    "hello",
                    json!({"ok": true, "user": {"uid": 60001}, "daemon": {"pid": 123},
                        "session": {"managerName": "Aqua", "onConsole": false},
                        "display": {"width": 100, "height": 100, "scale": 1}}),
                ),
                (
                    "run",
                    json!({"ok": true, "exitCode": 0, "timedOut": false,
                        "events": [{"event": "output", "data": STANDARD.encode(proof.to_string())}]}),
                ),
            ] {
                let (peer, _) = listener.accept().await.unwrap();
                let mut peer = BufReader::new(peer);
                let mut line = String::new();
                peer.read_line(&mut line).await.unwrap();
                assert_eq!(
                    serde_json::from_str::<Value>(&line).unwrap()["op"],
                    operation
                );
                peer.get_mut()
                    .write_all(format!("{reply}\n").as_bytes())
                    .await
                    .unwrap();
            }

            let (peer, _) = listener.accept().await.unwrap();
            let mut peer = BufReader::new(peer);
            let mut line = String::new();
            peer.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["op"], "run");
            assert_eq!(request["argv"], json!(["tool"]));
            assert_eq!(client.controller.pending().unwrap().len(), 1);
            client.controller.note_stop(true).unwrap();
            line.clear();
            assert_eq!(peer.read_line(&mut line).await.unwrap(), 0);
        };
        let running = client.run_owned_within(&ownership, argv, Duration::from_secs(timeout));
        let (outcome, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(running, helper)
        })
        .await
        .unwrap();
        let error = outcome.unwrap_err();
        assert_eq!(error.code, "stopped");
        assert!(error.uncertain);
        assert_eq!(client.controller.pending().unwrap().len(), 1);
        assert_eq!(client.controller.acquire(true).unwrap_err().code, "busy");
        drop(ownership);
        assert_eq!(
            client.controller.acquire(false).unwrap_err().code,
            "uncertain"
        );
    }

    #[test]
    fn a_stop_refuses_what_changes_the_helper_and_lets_looking_and_recovery_through() {
        let changes = |args: &[&str]| {
            let command = parse(args).unwrap().command.unwrap();
            changes_helper(&command, doing(&command).is_some())
        };

        for acting in [
            &["click", "1", "2"][..],
            &["type", "x"],
            &["key", "cmd+s"],
            &["open", "--app", "TextEdit"],
            &["close", "--app", "TextEdit"],
            &["run", "--", "ls"],
            &["input", "[]"],
            &["logout"],
            &["environment", "run", "--", "ls"],
            &["environment", "apply", "plan"],
            &["verify-native"],
            &["install"],
            &["update"],
            &["setup", "--apply"],
            &["setup", "--rollback"],
            &["environment", "rollback", "plan", "--apply"],
            &["environment", "profile", "profile.toml", "--apply"],
        ] {
            assert!(changes(acting), "{acting:?}");
        }

        for looking in [
            &["screenshot"][..],
            &["windows"],
            &["elements", "--app", "TextEdit"],
            &["restart"],
            &["start-input"],
            &["readiness", "--tool", "git"],
            &["stop"],
            &["environment"],
            &["environment", "rollback", "plan"],
            &["environment", "profile", "profile.toml"],
            &["update", "--check"],
            &["setup"],
        ] {
            assert!(!changes(looking), "{looking:?}");
        }
    }

    #[test]
    fn servers_helper_commands_and_setup_carry_no_host_session() {
        let attributed = |args: &[&str]| attributed(&parse(args).unwrap().command.unwrap());

        for unattributed in [
            &["mcp"][..],
            &["watch"],
            &["menubar"],
            &["monitor"],
            &["session-info"],
            &["permissions-info"],
            &["windows-info"],
            &["elements-info", "1"],
            &["init"],
            &["uninstall"],
            &["update"],
            &["setup"],
            &["switch"],
        ] {
            assert!(!attributed(unattributed), "{unattributed:?}");
        }

        for agents in [
            &["click", "1", "2"][..],
            &["type", "x"],
            &["screenshot"],
            &["open", "--app", "TextEdit"],
            &["close", "--app", "TextEdit"],
            &["run", "--", "ls"],
            &["environment"],
            &["activity"],
            &["agents"],
            &["agents", "status", "--brief"],
        ] {
            assert!(attributed(agents), "{agents:?}");
        }
    }

    #[test]
    fn only_an_environment_request_that_changes_the_helper_can_own_a_plan() {
        let kind = |args: &[&str]| kind(&parse(args).unwrap().command.unwrap());

        for changes in [
            &["environment", "profile.toml", "--apply"][..],
            &["environment", "apply", "plan"],
            &["environment", "rollback", "plan", "--apply"],
            &["environment", "profile", "profile.toml", "--apply"],
        ] {
            assert_eq!(
                kind(changes),
                Kind::Environment { changes: true },
                "{changes:?}"
            );
        }
        for looks in [
            &["environment", "profile.toml"][..],
            &["environment", "rollback", "plan"],
            &["environment", "profile", "profile.toml"],
        ] {
            assert_eq!(
                kind(looks),
                Kind::Environment { changes: false },
                "{looks:?}"
            );
        }
        for other in [
            &["environment", "discover"][..],
            &["environment", "run", "--", "ls"],
        ] {
            assert_eq!(kind(other), Kind::Other, "{other:?}");
        }
        assert!(!Kind::Other.recorded(None), "a read records nothing");
        assert!(Kind::Environment { changes: false }.recorded(None));
    }

    #[test]
    fn the_viewers_commands_never_carry_a_host_session() {
        for args in [
            &["click", "1", "2"][..],
            &["type", "x"],
            &["restart"],
            &["activity"],
        ] {
            let command = parse(args).unwrap().command.unwrap();
            assert!(labelled(&command, Actor::Agent), "{args:?}");
            assert!(!labelled(&command, Actor::Viewer), "{args:?}");
        }
    }

    #[test]
    fn only_commands_that_take_control_wait_and_listen_for_signals() {
        let waits = |args: &[&str]| controls(&parse(args).unwrap().command.unwrap());

        for controlling in [
            &["input", "[]"][..],
            &["click", "1", "2"],
            &["click", "--app", "Calculator", "--name", "7"],
            &["move", "1", "2"],
            &["drag", "1", "2", "3", "4"],
            &["scroll", "1", "2", "--dy", "3"],
            &["type", "x"],
            &["key", "cmd+s"],
            &["open", "--app", "TextEdit"],
            &["close", "--app", "TextEdit"],
            &["run", "--", "ls"],
            &["restart"],
            &["start-input"],
            &["install"],
            &["readiness", "--tool", "git"],
            &["environment"],
            &["environment", "run", "--", "ls"],
            &["verify-native"],
            &["logout", "--yes"],
        ] {
            assert!(waits(controlling), "{controlling:?}");
        }

        for at_once in [
            &["screenshot"][..],
            &["apps"],
            &["windows"],
            &["elements", "--app", "TextEdit"],
            &["wait", "--app", "TextEdit"],
            &["status"],
            &["doctor"],
            &["mcp"],
            &["watch"],
            &["menubar"],
            &["monitor"],
            &["init"],
            &["uninstall"],
            &["update"],
            &["setup", "--apply"],
            &["switch"],
            &["logout"],
        ] {
            assert!(!waits(at_once), "{at_once:?}");
        }
    }

    #[test]
    fn only_an_agent_waits_and_never_for_what_the_person_does_from_the_app() {
        for args in [
            &["type", "x"][..],
            &["run", "--", "ls"],
            &["restart"],
            &["logout", "--yes"],
        ] {
            let command = parse(args).unwrap().command.unwrap();
            assert!(waits(&command, Actor::Agent), "{args:?}");
            assert!(!waits(&command, Actor::Viewer), "{args:?}");
        }
        let looking = parse(&["screenshot"]).unwrap().command.unwrap();
        assert!(!waits(&looking, Actor::Agent));
    }

    #[test]
    fn setup_commands_refuse_to_run_as_root() {
        let error = refuse_root("update", true).unwrap_err();
        assert_eq!(error.error, "Run atlas update as yourself, not with sudo.");
        assert!(refuse_root("init", false).is_ok());
    }
}
