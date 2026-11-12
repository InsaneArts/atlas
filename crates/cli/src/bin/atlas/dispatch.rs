//! Command execution, activity attribution and helper-change policy.
use super::{
    cli::{
        self, AgentPicks, AgentsCommand, Command, CursorHookEvent, EnvironmentCommand, HomeFiles,
        JobCommand, OnOff, OnboardingCommand, SignInCopies, WindowState,
    },
    doctor, help,
    output::{self, print},
    policy::Policy,
    prompt, update,
};
use atlas_agents::{Change, Covering};
use atlas_cli::{menubar, ui::progress::Spinner, viewer};
use atlas_client::{
    agent,
    client::{Actor, Client},
    doing::Doing,
    environment,
    host_session::{self, store::Kind},
    task::{self, Task},
};
use atlas_core::{
    config::Config,
    desktop,
    error::{Code, Error, Result},
    transport::with_output,
};
use atlas_core::{host::HostSession, screen::frame::Size};
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

/// How large a screenshot is saved: at most one pixel per display point and
/// [`atlas_core::screen::frame::SCREENSHOT_SIDE`] pixels on its longer side unless a
/// longest side is given; 0 asks for the full size.
pub(super) fn screenshot_size(given: Option<u32>) -> Size {
    match given {
        None => Size::Standard,
        Some(0) => Size::Full,
        Some(side) => Size::Longest(side),
    }
}

/// Make `change` for the picked agents and Claude app; none picked means
/// every installed one, the Claude app included.
fn changed(picks: &AgentPicks, change: Change) -> Result<Value> {
    // Connecting an app that isn't here would only leave its settings behind.
    if picks.app && change == Change::Connect && !atlas_agents::claude_app::installed() {
        return Err(Error::new(Code::Agents, "The Claude app isn't installed"));
    }
    if picks.app {
        atlas_agents::change_these(
            &picks.agents(),
            Covering {
                every_agent: false,
                app: true,
            },
            change,
        )
        .map(|changed| changed.report)
    } else {
        atlas_agents::change(&picks.agents(), change)
    }
}

/// The task a command from host session `host` belongs to, as `name`
/// (ATLAS_TASK's value) names it. A name that is not a plain one refuses a
/// command that `makes` something, so its work is never filed under
/// another task; any other command goes on without a task.
fn task_of(makes: bool, host: Option<&HostSession>, name: Option<&OsStr>) -> Result<Option<Task>> {
    match Task::resolve(host, name) {
        Err(error) if makes => Err(error),
        Err(_) => Ok(None),
        resolved => resolved,
    }
}

#[cfg(target_os = "macos")]
fn elements_info(pid: i32) -> Result<Value> {
    let limits = atlas_core::screen::elements::Limits::default();
    let snapshot = atlas_native::native::ax::elements(pid, limits)?;
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
        Code::Platform,
        "Accessibility elements require macOS",
    ))
}

#[cfg(target_os = "macos")]
async fn input_service(service: cli::Service) -> Result<Value> {
    atlas_native::native::service::serve(service.uid, service.controller_uid, service.socket)
        .await?;
    Ok(Value::Null)
}

#[cfg(not(target_os = "macos"))]
async fn input_service(_: cli::Service) -> Result<Value> {
    Err(Error::new(
        Code::Platform,
        "The input service requires macOS",
    ))
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

/// Carry out `command`. When it [`waits`], it waits for up to `wait` in all
/// for other controllers to let go of the helper. An agent's command is
/// labelled with the host session it comes from, and what it left is noted
/// in that session's record; a refusal names the other sessions it can.
pub(super) async fn execute(
    config: PathBuf,
    command: Command,
    policy: &Policy,
    wait: Duration,
    allow_disrupt: bool,
) -> Result<Value> {
    let actor = actor();

    // Helper-side commands run inside the helper login and never read the
    // controller's configuration. A command that talks to the helper loads
    // it before anything else.
    let doing = policy.doing.clone();
    let changes = policy.changes_helper();
    let waits = policy.waits(actor);
    let host = if policy.labelled(actor) {
        match Config::load(&config).and_then(Client::new) {
            Ok(client) => atlas_client::host_session::from_env_verified(&client).await,
            Err(_) => HostSession::from_env(),
        }
    } else {
        None
    };
    let task = task_of(
        policy.makes(),
        host.as_ref(),
        std::env::var_os("ATLAS_TASK").as_deref(),
    )?;
    let client = || {
        Client::new(Config::load(&config)?).map(|mut client| {
            client.actor = actor;
            client.doing = doing.clone();
            client.host_session = host.clone();
            client.allow_disrupt = allow_disrupt;
            client.task = task.clone();
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

    execute_with(
        &config,
        command,
        policy,
        host.as_ref(),
        &client,
        allow_disrupt,
    )
    .await
}

/// Execute with the attributed session and its configured client.
async fn execute_with(
    config: &Path,
    command: Command,
    policy: &Policy,
    host: Option<&HostSession>,
    client: &impl Fn() -> Result<Client>,
    allow_disrupt: bool,
) -> Result<Value> {
    let doing = policy.doing.as_ref();
    let kind = policy.kind;

    // Its session is seen at once, whatever the command does or however it
    // ends: a label, so a record that cannot be written changes nothing.
    if let Some(host) = host
        && let Ok(client) = client()
    {
        let _ = host_session::store::touch(&client, host);
    }

    // The person's stop refuses an agent's change at its start; one already
    // under way is the input service's to cut off.
    if policy.changes_helper() {
        client()?.require_going().await?;
    }

    let result = carry_out(config, command, client, host, allow_disrupt).await;

    if kind.recorded(doing)
        && let Ok(client) = client()
    {
        note_outcome(&client, host, kind, doing, &result);
    }

    // A refusal that can name the other session, or say whose an unknown
    // outcome is, says so.
    host_session::explain::reply(result, || client().ok(), host)
}

/// Note what a command of `kind`, doing `doing`, left for its session to
/// find again once it ended, as `result` says, and a plan it rolled back as
/// undone wherever it is listed: labels, so a record that cannot be written
/// changes nothing.
fn note_outcome(
    client: &Client,
    host: Option<&HostSession>,
    kind: Kind,
    doing: Option<&Doing>,
    result: &Result<Value>,
) {
    let facts = host_session::store::ReplyFacts::new(kind, result);
    if let Some(host) = host {
        let _ = host_session::store::note_facts(client, host, doing, &facts);
    }
    let _ = host_session::store::forget_rolled_back_facts(client, &facts);
    let _ = task::settle_rolled_back_facts(client, &facts);
}

/// [`execute`]'s work once the command is labelled: `client` gives its
/// client, and `host` is its host session, if any. An arm that calls
/// `client` makes the client first; an arm that passes it on refuses what
/// it can before making one.
async fn carry_out(
    config: &Path,
    command: Command,
    client: &impl Fn() -> Result<Client>,
    host: Option<&HostSession>,
    allow_disrupt: bool,
) -> Result<Value> {
    match command {
        Command::EnvironmentWorker { packet, apply } => environment::worker(&packet, apply),
        Command::AgentGuide(args) => Ok(guide(&args)),
        Command::SessionInfo => atlas_native::native::session_info(),
        Command::SessionProofServe => {
            unreachable!("main serves independent proofs before dispatch")
        }
        Command::PermissionsInfo => Ok(atlas_daemon::daemon::facts::permissions_now()),
        Command::ElementsInfo { pid } => elements_info(pid),
        // Main runs the helper daemon on its own thread and becomes a job
        // before it creates the async runtime.
        Command::HelperServe(_) | Command::JobLaunch { .. } => {
            unreachable!("main runs these before anything else")
        }
        Command::InputService(args) => input_service(args).await,
        Command::SetupHelper(args) => setup_helper(&args),
        Command::Update(args) => update::run(config, args, host, allow_disrupt).await,
        Command::UpdateFinish => update::update_finish(config, host, allow_disrupt).await,
        Command::PictureAfter { before, ended } => {
            client()?.picture_after(&before, ended).await?;
            Ok(json!({"ok": true}))
        }
        Command::Init => super::init::run(config, allow_disrupt).await,
        Command::Onboarding { step } => onboarding(config, step).await,
        Command::Agents { action } => agents(config, host, action).await,
        Command::Uninstall(args) => super::uninstall::run(config, args).await,
        Command::Menubar(args) => menu_bar(config, client, args).await,
        Command::Switch(args) => switch(config, args),
        Command::Setup(args) => setup(config, client, args).await,
        Command::Config(args) => settings(config, client, &args),
        Command::Status => Ok(client()?.status().await),
        Command::Sound { state: None } => atlas_client::sound::show(&client()?).await,
        Command::Sound { state: Some(state) } => {
            atlas_client::sound::set(&client()?, state == cli::OnOff::Off).await
        }
        Command::Activity(args) => activity(&client()?, host, &args),
        Command::Doctor(args) if args.login_timing => atlas_core::login_log::timing(),
        Command::Doctor(args) => Ok(doctor::report(&client()?, args.verbose).await),
        Command::VerifyNative { fixture } => {
            atlas_client::capability::verify(&client()?, &fixture).await
        }
        Command::Environment(args) => provision(client, args).await,
        Command::Install => client()?.install(),
        Command::StartInput => client()?.start_input().await,
        Command::Watch { port, frames, once } => {
            atlas_cli::watch::watch(client()?, port, frames, once)
                .await
                .map(|()| Value::Null)
        }
        Command::Stop => client()?.hold(true).await,
        Command::Resume => client()?.hold(false).await,
        Command::Mcp(args) => mcp(config, client()?, args).await,
        Command::Monitor {
            terminal: true,
            read_only,
            max_dimension,
            capture_backend,
            ..
        } => {
            let backend = if capture_backend.as_deref() == Some("cg") {
                atlas_core::protocol::CaptureBackend::CoreGraphics
            } else {
                atlas_core::protocol::CaptureBackend::ScreenCaptureKit
            };
            atlas_cli::terminal::monitor(client()?, config, read_only, max_dimension, backend)
                .await?;
            Ok(Value::Null)
        }
        Command::Monitor { port, .. } => {
            viewer::monitor(config, port, client()?.output_dir()?).await
        }
        Command::Screenshot(args) => {
            let client = client()?;
            client.controller.shown(screenshot(&client, args)).await
        }
        Command::Apps { all } => Ok(atlas_client::app_selection::listed(
            client()?.apps().await?,
            all,
        )),
        Command::Click(args) => click(&client()?, args).await,
        Command::Elements(args) => {
            let client = client()?;
            client.controller.shown(elements(&client, args)).await
        }
        Command::Element(args) => {
            let action = args.as_input()?;
            agent::element(&client()?, args.target.target().as_ref(), &action).await
        }
        Command::Readiness { tools, apps } => {
            atlas_client::readiness::readiness(&client()?, &tools, &apps).await
        }
        Command::Move { x, y } => agent::act(&client()?, None, &agent::move_to(x, y)).await,
        Command::Drag(args) => drag(&client()?, args).await,
        Command::Scroll(args) => scroll(client, args).await,
        Command::Type(args) => type_text(client, args).await,
        Command::Key(args) => press_keys(client, args).await,
        Command::Windows { app, title, all } => {
            let client = client()?;
            let filter = agent::WindowFilter { app, title, all };
            let listed = agent::list(&client, &filter);
            client
                .controller
                .shown(listed)
                .await
                .map(|windows| json!({"ok": true, "windows": windows}))
        }
        Command::Open(args) => open(&client()?, &args).await,
        Command::InstallApp { path } => atlas_client::install_app::install(&client()?, &path).await,
        Command::Cleanup(args) => cleanup(&client()?, &args).await,
        Command::Close { app, .. } => close(&client()?, app.as_deref()).await,
        Command::Wait(args) => {
            let client = client()?;
            client.controller.shown(wait(&client, args)).await
        }
        Command::Input { actions, target } => input(&client()?, actions, target).await,
        Command::Job { command } => job(&client()?, command).await,
        Command::Collect { files, max_size } => {
            atlas_client::collect::collect(&client()?, &files, max_size).await
        }
        // Control is held only while the command starts: other agents may
        // act while it runs.
        Command::Run(args) => client()?
            .run_beside(args.argv, Duration::from_secs(args.timeout))
            .await
            .map(with_output),
        Command::Restart => restart(&client()?).await,
        Command::Logout { force, yes } => logout(&client()?, force, yes).await,
    }
}

fn guide(args: &cli::AgentGuide) -> Value {
    let guide = match (args.skill, args.commands, args.errors) {
        (true, _, _) => atlas_agents::guide::skill(),
        (_, true, _) => help::command_reference(),
        (_, _, true) => atlas_agents::guide::ERRORS.to_owned(),
        _ => atlas_agents::guide::GUIDE.to_owned(),
    };
    print(&guide);
    Value::Null
}

fn setup_helper(args: &cli::SetupHelper) -> Result<Value> {
    let guard = atlas_daemon::helper_setup::Guard {
        account: Some(serde_json::from_str(&args.account)?),
        logged_out: args.logged_out,
    };
    atlas_daemon::helper_setup::run_guarded(serde_json::from_str(&args.request)?, &guard)
}

/// `atlas setup`: a preview, or the recorded setup applied.
async fn setup(
    config: &Path,
    client: &impl Fn() -> Result<Client>,
    args: cli::Setup,
) -> Result<Value> {
    if !args.apply {
        return atlas_client::setup::preview(config, args.helper.as_deref()).await;
    }

    let client = client()?;
    // Setup settles only mutations its own log records, and refuses others.
    let ownership = client.service_change(true).await?;
    atlas_client::setup::migrate::apply(config, &client, &ownership).await
}

fn activity(client: &Client, host: Option<&HostSession>, args: &cli::Activity) -> Result<Value> {
    use host_session::activity::Scope;

    let scope = match (args.session.as_ref(), args.all) {
        (Some(key), _) => Scope::Session(key),
        (None, true) => Scope::Everyone,
        (None, false) => Scope::Own,
    };
    host_session::activity::list(client, host, scope, args.limit.into())
}

/// `atlas environment`: a manifest's copy, or one of its commands.
async fn provision(client: &impl Fn() -> Result<Client>, args: cli::Environment) -> Result<Value> {
    if let Some(command) = args.command {
        return environment_command(&client()?, command).await;
    }

    let manifest = args.manifest.ok_or_else(|| {
        Error::new(
            Code::Usage,
            "atlas environment needs a profile file, or a command such as discover or run.",
        )
    })?;
    environment::sync(&client()?, &manifest, args.apply).await
}

async fn mcp(config: &Path, client: Client, args: cli::Mcp) -> Result<Value> {
    atlas_cli::mcp::serve(
        client,
        config.to_path_buf(),
        args.port,
        args.read_only,
        args.guard_app_controls,
    )
    .await?;
    Ok(Value::Null)
}

async fn screenshot(client: &Client, args: cli::Screenshot) -> Result<Value> {
    agent::screenshot(
        client,
        args.target.target().as_ref(),
        args.region,
        args.window,
        screenshot_size(args.max_dimension),
        args.new_file,
    )
    .await
}

/// `atlas click`: at X Y, on the element a reference names, or on the
/// element its filter finds in its target's app.
async fn click(client: &Client, args: cli::Click) -> Result<Value> {
    let button = args.button.into();

    if let Some(reference) = &args.reference {
        return agent::click_reference(
            client,
            args.target.target().as_ref(),
            reference,
            button,
            args.count,
            &args.modifiers,
        )
        .await;
    }

    match ((args.x, args.y), args.target.target()) {
        ((Some(x), Some(y)), target) => {
            agent::act(
                client,
                agent::Expected::new(target, args.window, None)?.as_ref(),
                &agent::click(x, y, button, args.count, &args.modifiers),
            )
            .await
        }
        ((None, None), Some(target)) => {
            let filter = atlas_core::screen::select::ElementFilter {
                role: args.role,
                name: args.name,
            };
            agent::click_element(
                client,
                &target,
                &filter,
                button,
                args.count,
                &args.modifiers,
            )
            .await
        }
        _ => Err(Error::new(
            Code::Usage,
            "Give X Y, or --app/--pid with --name/--role",
        )),
    }
}

async fn elements(client: &Client, args: cli::Elements) -> Result<Value> {
    let target = args
        .target
        .target()
        .ok_or_else(|| Error::new(Code::Usage, "Select the app with --app or --pid"))?;
    let form = match (args.values, args.full) {
        (true, _) => agent::Form::Values,
        (false, true) => agent::Form::Full,
        (false, false) => agent::Form::Compact,
    };
    let asked = agent::Asked {
        role: args.role,
        names: args.name,
        window: args.window,
        page: args.page,
        max: args.max,
        form,
    };
    agent::elements(client, &target, &asked).await
}

async fn drag(client: &Client, args: cli::Drag) -> Result<Value> {
    agent::act(
        client,
        args.target.expected(args.window, None)?.as_ref(),
        &agent::drag(
            (args.from_x, args.from_y),
            (args.to_x, args.to_y),
            args.button.into(),
        ),
    )
    .await
}

async fn scroll(client: &impl Fn() -> Result<Client>, args: cli::Scroll) -> Result<Value> {
    if args.dx == 0 && args.dy == 0 {
        return Err(Error::new(
            Code::Usage,
            "Give lines to scroll with --dy (down) or --dx (right), negative for up or left",
        ));
    }
    agent::act(
        &client()?,
        args.target.expected(args.window, None)?.as_ref(),
        &agent::scroll((args.x, args.y), (args.dx, args.dy)),
    )
    .await
}

async fn type_text(client: &impl Fn() -> Result<Client>, args: cli::Type) -> Result<Value> {
    let expected = args.target.expected(args.window, args.field)?;
    let client = client()?;
    let text = text_to_type(args.text, args.stdin)?;
    agent::type_text(&client, &text, expected.as_ref()).await
}

async fn press_keys(client: &impl Fn() -> Result<Client>, args: cli::Keys) -> Result<Value> {
    let expected = args.target.expected(args.window, args.field)?;
    agent::act(&client()?, expected.as_ref(), &agent::keys(&args.keys)?).await
}

async fn open(client: &Client, args: &cli::Open) -> Result<Value> {
    agent::open(
        client,
        args.app.as_deref(),
        &args.targets,
        &args.environment,
    )
    .await
}

async fn cleanup(client: &Client, args: &cli::Cleanup) -> Result<Value> {
    let mode = if args.apply {
        atlas_client::cleanup::Mode::Apply
    } else {
        atlas_client::cleanup::Mode::Preview
    };
    atlas_client::cleanup::cleanup(client, args.session.as_ref(), args.task.as_ref(), mode).await
}

async fn close(client: &Client, app: Option<&str>) -> Result<Value> {
    agent::close(client, app.map_or(agent::Closing::All, agent::Closing::App)).await
}

async fn wait(client: &Client, args: cli::Wait) -> Result<Value> {
    if args.name.is_some() || args.role.is_some() {
        return wait_for_element(client, args).await;
    }
    let filter = agent::WindowFilter {
        app: args.app,
        title: args.title,
        all: false,
    };
    agent::wait(
        client,
        &filter,
        args.gone,
        Duration::from_secs(args.timeout),
    )
    .await
}

/// `atlas wait` with --name or --role: an element of the app, by the state asked for.
async fn wait_for_element(client: &Client, args: cli::Wait) -> Result<Value> {
    use atlas_core::screen::wait::State;

    let target = cli::AppTarget {
        app: args.app,
        pid: args.pid,
    }
    .target()
    .ok_or_else(|| {
        Error::new(
            Code::Usage,
            "Wait for an element of an app: add --app or --pid",
        )
    })?;
    let state = match (args.value, args.state, args.gone) {
        (Some(value), ..) => State::Value(value),
        (None, Some(cli::ElementState::Gone), _) | (None, None, true) => State::Gone,
        (None, Some(cli::ElementState::Exists), _) | (None, None, false) => State::Exists,
        (None, Some(cli::ElementState::Enabled), _) => State::Enabled,
        (None, Some(cli::ElementState::Visible), _) => State::Visible,
    };
    let filter = atlas_core::screen::select::ElementFilter {
        role: args.role,
        name: args.name,
    };
    agent::wait_for_element(
        client,
        &target,
        &filter,
        &state,
        Duration::from_secs(args.timeout),
    )
    .await
}

async fn input(client: &Client, actions: String, target: cli::AppTarget) -> Result<Value> {
    let bytes = actions_batch(actions)?;
    agent::act(
        client,
        target.expected(None, None)?.as_ref(),
        &serde_json::from_slice(&bytes)?,
    )
    .await
}

async fn restart(client: &Client) -> Result<Value> {
    let _spinner = Spinner::new("Restarting the helper…");
    client.restart().await
}

/// The text `atlas type` types: as given, or read from stdin, at most 1 MiB.
fn text_to_type(text: Option<String>, stdin: bool) -> Result<String> {
    const LIMIT: u64 = 1 << 20;

    match text {
        Some(text) if !stdin => Ok(text),
        _ => {
            let mut text = String::new();
            std::io::stdin().take(LIMIT + 1).read_to_string(&mut text)?;
            if text.len() as u64 > LIMIT {
                return Err(Error::new(Code::Input, "Text from stdin exceeds 1 MiB"));
            }
            Ok(text)
        }
    }
}

/// `atlas onboarding`: the steps Atlas.app's setup window runs.
async fn onboarding(config: &Path, step: OnboardingCommand) -> Result<Value> {
    use atlas_cli::onboarding;

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
        OnboardingCommand::AcceptWorker { ticket } => onboarding::accept_worker(&ticket).await,
        OnboardingCommand::Window { state } => onboarding::window(state == WindowState::Open),
        OnboardingCommand::Forget => onboarding::forget(config),
        OnboardingCommand::Close { helper: false } => onboarding::close_leftovers(config),
        OnboardingCommand::Close { helper: true } => onboarding::close_helper(config),
        OnboardingCommand::Finish => onboarding::finish(config).await,
        OnboardingCommand::Login { state } => onboarding::login(config, state == OnOff::On),
        OnboardingCommand::Files { change } => {
            let change = change.map(|change| match change {
                HomeFiles::Private => onboarding::HomeChange::Private,
                HomeFiles::Open => onboarding::HomeChange::Open,
                HomeFiles::Offered => onboarding::HomeChange::Offered,
            });
            let (config, by_person) = (config.to_path_buf(), actor() == Actor::Viewer);
            // `ls` and `chmod` wait on their own, off the command's async work.
            tokio::task::spawn_blocking(move || onboarding::files(&config, change, by_person))
                .await
                .map_err(|error| Error::new(Code::Io, error.to_string()))?
        }
        OnboardingCommand::SignIns { choice } => onboarding::sign_ins(
            choice.map(|choice| choice == SignInCopies::Allow),
            actor() == Actor::Viewer,
        ),
    }
}

/// `atlas agents`: which agents are connected, connecting and
/// disconnecting them, a project's section, and Cursor's hook.
async fn agents(
    config: &Path,
    host: Option<&HostSession>,
    action: Option<AgentsCommand>,
) -> Result<Value> {
    match action.unwrap_or(AgentsCommand::Status { brief: false }) {
        AgentsCommand::Status { brief: true } => {
            print(&format!("{}\n", atlas_agents::brief(config, host).await));
            Ok(Value::Null)
        }
        AgentsCommand::Status { brief: false } => {
            let _spinner = Spinner::new("Checking agent connections…");
            Ok(atlas_agents::status())
        }
        AgentsCommand::CursorHook { event } => {
            let event = match event {
                CursorHookEvent::SessionStart => "session-start",
                CursorHookEvent::BeforeShell => "before-shell",
            };
            let reply = atlas_agents::hook(config, event, std::io::stdin().lock()).await;
            print(&format!("{reply}\n"));
            Ok(Value::Null)
        }
        AgentsCommand::Project { dir } => {
            atlas_agents::project(&dir.unwrap_or_else(|| PathBuf::from(".")))
        }
        AgentsCommand::Forget { dir } => {
            atlas_agents::forget(&dir.unwrap_or_else(|| PathBuf::from(".")))
        }
        AgentsCommand::Install { picks } => {
            let _spinner = Spinner::new("Connecting…");
            changed(&picks, Change::Connect)
        }
        AgentsCommand::Remove { picks } => {
            let _spinner = Spinner::new("Disconnecting…");
            changed(&picks, Change::Disconnect)
        }
    }
}

/// `atlas environment <command>`: a plan's steps, its runs and its list.
async fn environment_command(client: &Client, command: EnvironmentCommand) -> Result<Value> {
    match command {
        EnvironmentCommand::Profile { profile, apply } => {
            environment::profile::sync(client, &profile, apply).await
        }
        EnvironmentCommand::Run(args) => {
            environment::run(client, args.argv, Duration::from_secs(args.timeout))
                .await
                .map(with_output)
        }
        EnvironmentCommand::Apply { plan_id } => environment::apply(client, &plan_id).await,
        EnvironmentCommand::List { all } => environment::list(client, all).await,
        EnvironmentCommand::Rollback {
            transaction_id,
            apply,
        } => environment::rollback(client, &transaction_id, apply).await,
        EnvironmentCommand::Discover {
            tools,
            apps,
            environment,
            references,
        } => {
            environment::discovery::discover(client, &tools, &apps, &environment, &references).await
        }
    }
}

/// `atlas menubar`: its login item, or draw its menu into a folder, serve
/// it (when asked, or once started at login), or start it in the background.
async fn menu_bar(
    config: &Path,
    client: &impl Fn() -> Result<Client>,
    args: cli::Menubar,
) -> Result<Value> {
    if let Some(login) = args.login {
        return menubar::set_login(login);
    }

    let client = client()?;
    let program = menubar::Program::new(config)?;

    match args.render {
        Some(directory) => viewer::render_menubar(client, &program, &directory).await,
        None if args.serve || menubar::started_at_login() => {
            // Its own session: closing the terminal that started it does
            // not hang it up.
            if args.serve {
                let _ = nix::unistd::setsid();
            }
            viewer::serve(client, args.port, program, output::serving_menu_bar).await
        }
        None => {
            let output = client.output_dir()?.to_path_buf();
            let (already, record) = menubar::launch(config, args.port, &output).await?;
            output::started_menu_bar(already, &record);
            Ok(Value::Null)
        }
    }
}

/// `atlas job`: start, list, read and stop background jobs.
async fn job(client: &Client, command: JobCommand) -> Result<Value> {
    match command {
        JobCommand::Start {
            name,
            timeout,
            argv,
        } => atlas_client::jobs::start(client, argv, Duration::from_secs(timeout), name).await,
        JobCommand::List { all } => atlas_client::jobs::list(client, all).await,
        JobCommand::Log { id, bytes, task } => {
            atlas_client::jobs::log(client, &id, bytes, task.as_ref()).await
        }
        JobCommand::Stop { id, grace, task } => {
            atlas_client::jobs::stop(client, &id, Duration::from_secs(grace), task.as_ref()).await
        }
    }
}

/// `atlas logout`: log the helper out once the person agrees, unless `yes`.
async fn logout(client: &Client, force: bool, yes: bool) -> Result<Value> {
    if !yes
        && !prompt::confirm(
            "Log out the helper? Its apps quit, and agents can't use it until you log in to it \
             again.",
            false,
            "pass --yes",
        )?
    {
        return Err(prompt::cancelled());
    }

    let _spinner = Spinner::new("Logging out the helper…");
    client.logout(force).await
}

/// An actions batch as given, or read from the file `@path` names: at most
/// 32 KiB.
fn actions_batch(actions: String) -> Result<Vec<u8>> {
    const LIMIT: u64 = 32 * 1024;

    let bytes = match actions.strip_prefix('@') {
        Some(path) => {
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .map_err(|error| Error::new(Code::Io, format!("Could not read {path}: {error}")))?
                .take(LIMIT + 1)
                .read_to_end(&mut bytes)?;
            bytes
        }
        None => actions.into_bytes(),
    };
    if bytes.len() as u64 > LIMIT {
        return Err(Error::new(
            Code::Input,
            "Input exceeds 32 KiB; use short observed batches",
        ));
    }
    Ok(bytes)
}

/// `atlas config`: change the settings and the pictures as `args` asks,
/// then show the settings, with the pictures kept when asked.
fn settings(
    config: &Path,
    client: &impl Fn() -> Result<Client>,
    args: &cli::Config,
) -> Result<Value> {
    if let Some(state) = args.pictures_before_actions {
        atlas_core::config::keep_pictures(config, state == cli::OnOff::On)?;
    }
    if let Some(days) = args.keep_pictures_days {
        atlas_core::config::keep_days(config, days)?;
    }
    if let Some(limit) = args.typed_text_limit {
        atlas_core::config::typed_text_limit(config, (limit > 0).then_some(limit))?;
    }
    let client = client()?;
    let output = &client.config.output_dir;
    if args.delete_pictures {
        atlas_client::before::delete_all(output)?;
    }
    let mut reply = json!({ "ok": true, "config": client.config, "path": config });
    if args.pictures_summary {
        let (count, bytes) = atlas_client::before::summary(output);
        reply["pictures"] = json!({"count": count, "bytes": bytes});
    }
    Ok(reply)
}

/// Lock the screen so the person can switch to the helper (or an account
/// named). macOS asks for that account's password on the lock screen; there
/// is nothing to choose here beyond whether to lock now.
fn switch(config: &Path, args: cli::Switch) -> Result<Value> {
    use atlas_client::switch;

    let helper = Config::load(config)
        .ok()
        .map(|config| match config.desktop().kind() {
            desktop::Kind::SecondLogin(login) => login.uid,
        });
    let accounts = switch::accounts(helper)?;

    if args.list {
        return Ok(json!({"ok": true, "accounts": accounts}));
    }

    let target = match args.account {
        Some(name) => Some(
            accounts
                .iter()
                .find(|candidate| candidate.name == name)
                .cloned()
                .ok_or_else(|| {
                    Error::new(
                        Code::Switch,
                        format!("No account named {name} to switch to"),
                    )
                })?,
        ),
        None if args.lock => None,
        None => accounts.iter().find(|candidate| candidate.helper).cloned(),
    };

    if !args.yes {
        let question = match &target {
            Some(account) => format!(
                "Lock the screen so you can switch to {}?",
                account.real_name.as_deref().unwrap_or(&account.name)
            ),
            None => "Lock the screen now?".to_owned(),
        };

        if !prompt::confirm(&question, true, "pass --yes")? {
            return Err(prompt::cancelled());
        }
    }

    match target {
        Some(account) => switch::to(&account),
        None => switch::lock_screen(),
    }
}

/// Refuse `atlas <command>` run as root (with sudo): it would act for root's
/// home, not the person's, and leave files only root can change.
pub(super) fn as_yourself(command: &str) -> Result<()> {
    refuse_root(command, nix::unistd::geteuid().is_root())
}

fn refuse_root(command: &str, root: bool) -> Result<()> {
    if root {
        return Err(Error::new(
            Code::Usage,
            format!("Run atlas {command} as yourself, not with sudo."),
        ));
    }

    Ok(())
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
