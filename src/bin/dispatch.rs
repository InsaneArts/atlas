//! Command execution, activity attribution and helper-change policy.
use super::{
    cli::{
        AgentPicks, AgentsCommand, AppTarget, Command, CursorHookEvent, EnvironmentCommand,
        JobCommand, OnOff, OnboardingCommand, WindowState,
    },
    doctor, help,
    output::print,
    policy::Policy,
    update,
};
use atlas::{
    agent,
    agents::{Change, Covering},
    client::{Actor, Client, Size},
    config::Config,
    doing::Doing,
    environment,
    error::{Code, Error, Result},
    host_session::{self, HostSession, store::Kind},
    menubar,
    task::{self, Task},
    transport::with_output,
    ui::{self, progress::Spinner},
    viewer,
};
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
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

/// Make `change` for the picked agents and Claude app; none picked means
/// every installed one, the Claude app included.
fn changed(picks: &AgentPicks, change: Change) -> Result<Value> {
    // Connecting an app that isn't here would only leave its settings behind.
    if picks.app && change == Change::Connect && !atlas::claude_app::installed() {
        return Err(Error::new(Code::Agents, "The Claude app isn't installed"));
    }
    if picks.app {
        atlas::agents::change_these(
            &picks.agents(),
            Covering {
                every_agent: false,
                app: true,
            },
            change,
        )
    } else {
        atlas::agents::change(&picks.agents(), change)
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

/// The helper's on-screen windows and, with `dock`, where its Dock takes
/// clicks, as one reading for the controller to judge points by. Only a
/// check of a point asks for the Dock, which is read through Accessibility.
#[cfg(target_os = "macos")]
fn windows_info(dock: bool) -> Result<Value> {
    use atlas::native::{ax, windows};

    let windows = windows::windows()?;
    let mut info = json!({ "ok": true, "windows": windows });
    if dock {
        info["dock"] = json!(ax::dock(&windows, ax::DOCK_TIMEOUT));
    }
    Ok(info)
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
        Code::Platform,
        "Accessibility elements require macOS",
    ))
}

#[cfg(not(target_os = "macos"))]
fn windows_info(_: bool) -> Result<Value> {
    Err(Error::new(Code::Platform, "Window listing requires macOS"))
}

#[cfg(target_os = "macos")]
async fn input_service(uid: u32, controller_uid: u32, socket: PathBuf) -> Result<Value> {
    atlas::native::service::serve(uid, controller_uid, socket).await?;
    Ok(Value::Null)
}

#[cfg(not(target_os = "macos"))]
async fn input_service(_: u32, _: u32, _: PathBuf) -> Result<Value> {
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
) -> Result<Value> {
    let actor = actor();

    // Helper-side commands run inside the helper login and never read the
    // controller's configuration. A command that talks to the helper loads
    // it before anything else.
    let doing = policy.doing.clone();
    let kind = policy.kind;
    let changes = policy.changes_helper;
    let waits = policy.waits(actor);
    let host = policy.labelled(actor).then(HostSession::from_env).flatten();
    let task = task_of(
        policy.makes,
        host.as_ref(),
        std::env::var_os("ATLAS_TASK").as_deref(),
    )?;
    let client = || {
        Client::new(Config::load(&config)?).map(|mut client| {
            client.actor = actor;
            client.doing = doing.clone();
            client.host_session = host.clone();
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

    // Its session is seen at once, whatever the command does or however it
    // ends: a label, so a record that cannot be written changes nothing.
    if let Some(host) = &host
        && let Ok(client) = client()
    {
        let _ = host_session::store::touch(&client, host);
    }

    // The person's stop refuses an agent's change at its start; one already
    // under way is the input service's to cut off.
    if changes {
        client()?.require_going().await?;
    }

    let result = carry_out(&config, command, &client, host.as_ref()).await;

    if kind.recorded(doing.as_ref())
        && let Ok(client) = client()
    {
        note_outcome(&client, host.as_ref(), kind, doing.as_ref(), &result);
    }

    // A refusal that can name the other session, or say whose an unknown
    // outcome is, says so.
    host_session::explain::reply(result, || client().ok(), host.as_ref())
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
    if let Some(host) = host {
        let _ = host_session::store::note(client, host, kind, doing, result);
    }
    let _ = host_session::store::forget_rolled_back(client, kind, result);
    let _ = task::settle_rolled_back(client, kind, result);
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
            let guide = match (skill, commands, errors) {
                (true, _, _) => agent::skill(),
                (_, true, _) => help::command_reference(),
                (_, _, true) => agent::ERRORS.to_owned(),
                _ => agent::GUIDE.to_owned(),
            };
            print(&guide);
            Ok(Value::Null)
        }
        Command::SessionInfo => atlas::native::session_info(),
        Command::PermissionsInfo => Ok(atlas::daemon::facts::permissions_now()),
        Command::WindowsInfo { dock } => windows_info(dock),
        Command::ElementsInfo { pid } => elements_info(pid),
        Command::HelperServe { .. } => {
            unreachable!("main serves the helper daemon on its own thread")
        }
        Command::JobLaunch { .. } => {
            unreachable!("main becomes a background job before anything else")
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
        Command::Onboarding { step } => onboarding(config, step).await,
        Command::Agents { action } => {
            agents(
                config,
                host,
                action.unwrap_or(AgentsCommand::Status { brief: false }),
            )
            .await
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
        } => {
            use host_session::activity::Scope;

            let scope = match (session.as_ref(), all) {
                (Some(key), _) => Scope::Session(key),
                (None, true) => Scope::Everyone,
                (None, false) => Scope::Own,
            };
            host_session::activity::list(&client()?, host, scope, limit.into())
        }
        Command::Doctor {
            login_timing: true, ..
        } => atlas::login_log::timing(),
        Command::Doctor { verbose, .. } => Ok(doctor(&client()?, verbose).await),
        Command::VerifyNative { fixture } => atlas::capability::verify(&client()?, &fixture).await,
        Command::Environment {
            command: Some(command),
            ..
        } => environment_command(&client()?, command).await,
        Command::Environment {
            manifest,
            apply,
            command: None,
        } => {
            let manifest = manifest.ok_or_else(|| {
                Error::new(
                    Code::Usage,
                    "atlas environment needs a profile file, or a command such as discover or run.",
                )
            })?;
            environment::sync(&client()?, &manifest, apply).await
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
            viewer::monitor(config, port, client.output_dir()?).await
        }
        Command::Menubar {
            port,
            render,
            serve,
            ..
        } => menu_bar(config, client()?, port, render, serve).await,
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
        Command::Apps { all } => client()?
            .apps()
            .await
            .map(|reply| atlas::app_selection::listed(reply, all)),
        Command::Click {
            x,
            y,
            target,
            window,
            name,
            role,
            button,
            count,
            modifiers,
        } => {
            let filter = agent::ElementFilter { role, name };
            let press = Press {
                button: button.into(),
                count,
                modifiers,
            };
            click(&client()?, (x, y), target, window, &filter, &press).await
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
                .ok_or_else(|| Error::new(Code::Usage, "Select the app with --app or --pid"))?;
            let filter = agent::ElementFilter { role, name };
            agent::elements(&client, &target, &filter, max, full).await
        }
        Command::Readiness { tools, apps } => {
            atlas::readiness::readiness(&client()?, &tools, &apps).await
        }
        Command::Move { x, y } => agent::act(&client()?, None, &agent::move_to(x, y)).await,
        Command::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
            target,
            window,
        } => {
            agent::act(
                &client()?,
                target.expected(window, None)?.as_ref(),
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
            window,
        } => {
            if dx == 0 && dy == 0 {
                return Err(Error::new(
                    Code::Usage,
                    "Give lines to scroll with --dy (down) or --dx (right), negative for up or left",
                ));
            }

            agent::act(
                &client()?,
                target.expected(window, None)?.as_ref(),
                &agent::scroll((x, y), (dx, dy)),
            )
            .await
        }
        Command::Type {
            text,
            stdin,
            target,
            window,
            field,
        } => {
            let expected = target.expected(window, field)?;
            let client = client()?;
            let text = text_to_type(text, stdin)?;
            agent::type_text(&client, &text, expected.as_ref()).await
        }
        Command::Key {
            keys,
            target,
            window,
            field,
        } => {
            let expected = target.expected(window, field)?;
            agent::act(&client()?, expected.as_ref(), &agent::keys(&keys)?).await
        }
        Command::Windows { app, title, all } => {
            agent::list(&client()?, &agent::WindowFilter { app, title, all }).await
        }
        Command::Open {
            app,
            targets,
            environment,
        } => agent::open(&client()?, app.as_deref(), &targets, &environment).await,
        Command::InstallApp { path } => atlas::install_app::install(&client()?, &path).await,
        Command::Cleanup {
            apply,
            session,
            task,
        } => {
            let mode = if apply {
                atlas::cleanup::Mode::Apply
            } else {
                atlas::cleanup::Mode::Preview
            };
            atlas::cleanup::cleanup(&client()?, session.as_ref(), task.as_ref(), mode).await
        }
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
            let bytes = actions_batch(actions)?;
            agent::act(
                &client,
                target.expected(None, None)?.as_ref(),
                &serde_json::from_slice(&bytes)?,
            )
            .await
        }
        Command::Job { command } => job(&client()?, command).await,
        Command::Collect { files, max_size } => {
            atlas::collect::collect(&client()?, &files, max_size).await
        }
        // Control is held only while the command starts: other agents may
        // act while it runs.
        Command::Run { argv, timeout } => client()?
            .run_beside(argv, Duration::from_secs(timeout))
            .await
            .map(with_output),
        Command::Restart => {
            let client = client()?;
            let _spinner = Spinner::new("Restarting the helper…");
            client.restart().await
        }
        Command::Logout { force, yes } => logout(&client()?, force, yes).await,
    }
}

/// Which button a click presses, how often, and with which keys held.
struct Press {
    button: atlas::actions::Button,
    count: u8,
    modifiers: Vec<String>,
}

/// `atlas click`: at X Y, or on the element `filter` finds in `target`'s
/// app.
async fn click(
    client: &Client,
    at: (Option<f64>, Option<f64>),
    target: AppTarget,
    window: Option<i64>,
    filter: &agent::ElementFilter,
    press: &Press,
) -> Result<Value> {
    match (at, target.target()) {
        ((Some(x), Some(y)), target) => {
            agent::act(
                client,
                agent::Expected::new(target, window, None)?.as_ref(),
                &agent::click(x, y, press.button, press.count, &press.modifiers),
            )
            .await
        }
        ((None, None), Some(target)) => {
            agent::click_element(
                client,
                &target,
                filter,
                press.button,
                press.count,
                &press.modifiers,
            )
            .await
        }
        _ => Err(Error::new(
            Code::Usage,
            "Give X Y, or --app/--pid with --name/--role",
        )),
    }
}

/// The text `atlas type` types: as given, or read from stdin, at most 1 MiB.
fn text_to_type(text: Option<String>, stdin: bool) -> Result<String> {
    const LIMIT: u64 = 1 << 20;

    match text {
        Some(text) if !stdin => Ok(text),
        _ => {
            let mut text = String::new();
            read_at_most(std::io::stdin(), LIMIT).read_to_string(&mut text)?;
            if text.len() as u64 > LIMIT {
                return Err(Error::new(Code::Input, "Text from stdin exceeds 1 MiB"));
            }
            Ok(text)
        }
    }
}

/// `atlas onboarding`: the steps Atlas.app's setup window runs.
async fn onboarding(config: &Path, step: OnboardingCommand) -> Result<Value> {
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
        OnboardingCommand::AcceptWorker { ticket } => onboarding::accept_worker(&ticket).await,
        OnboardingCommand::Window { state } => onboarding::window(state == WindowState::Open),
        OnboardingCommand::Forget => onboarding::forget(config),
        OnboardingCommand::Close { helper: false } => onboarding::close_leftovers(config),
        OnboardingCommand::Close { helper: true } => onboarding::close_helper(config),
        OnboardingCommand::Finish => onboarding::finish(config).await,
        OnboardingCommand::Login { state } => onboarding::login(config, state == OnOff::On),
    }
}

/// `atlas agents`: which agents are connected, connecting and
/// disconnecting them, a project's section, and Cursor's hook.
async fn agents(config: &Path, host: Option<&HostSession>, action: AgentsCommand) -> Result<Value> {
    match action {
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
        EnvironmentCommand::Run { argv, timeout } => {
            environment::run(client, argv, Duration::from_secs(timeout))
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

/// `atlas menubar`: draw its menu into `render`, serve it (when `serve`,
/// or once started at login), or start it in the background.
async fn menu_bar(
    config: &Path,
    client: Client,
    port: u16,
    render: Option<PathBuf>,
    serve: bool,
) -> Result<Value> {
    let program = menubar::Program::new(config)?;

    match render {
        Some(directory) => viewer::render_menubar(client, &program, &directory).await,
        None if serve || menubar::started_at_login() => {
            // Its own session: closing the terminal that started it does
            // not hang it up.
            if serve {
                let _ = nix::unistd::setsid();
            }
            viewer::serve(client, port, program).await
        }
        None => {
            let output = client.output_dir()?.to_path_buf();
            menubar::start(config, port, &output).await?;
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
        } => atlas::jobs::start(client, argv, Duration::from_secs(timeout), name).await,
        JobCommand::List { all } => atlas::jobs::list(client, all).await,
        JobCommand::Log { id, bytes, task } => {
            atlas::jobs::log(client, &id, bytes, task.as_ref()).await
        }
        JobCommand::Stop { id, grace, task } => {
            atlas::jobs::stop(client, &id, Duration::from_secs(grace), task.as_ref()).await
        }
    }
}

/// `atlas logout`: log the helper out once the person agrees, unless `yes`.
async fn logout(client: &Client, force: bool, yes: bool) -> Result<Value> {
    if !yes
        && !ui::prompt::confirm(
            "Log out the helper? Its apps quit, and agents can't use it until you log in to it \
             again.",
            false,
            "pass --yes",
        )?
    {
        return Err(ui::prompt::cancelled());
    }

    let _spinner = Spinner::new("Logging out the helper…");
    client.logout(force).await
}

/// An actions batch as given, or read from the file `@path` names: at most
/// 32 KiB.
fn actions_batch(actions: String) -> Result<Vec<u8>> {
    const LIMIT: u64 = 32 * 1024;
    let too_long = || {
        Error::new(
            Code::Input,
            "Input exceeds 32 KiB; use short observed batches",
        )
    };

    let bytes = match actions.strip_prefix('@') {
        Some(path) => {
            let mut bytes = Vec::new();
            let file = std::fs::File::open(path)
                .map_err(|error| Error::new(Code::Io, format!("Could not read {path}: {error}")))?;
            read_at_most(file, LIMIT).read_to_end(&mut bytes)?;
            bytes
        }
        None => actions.into_bytes(),
    };
    if bytes.len() as u64 > LIMIT {
        return Err(too_long());
    }
    Ok(bytes)
}

/// `reader`, read no further than one byte past `limit`: enough to tell
/// that what it holds is too long.
fn read_at_most(reader: impl Read, limit: u64) -> impl Read {
    reader.take(limit + 1)
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
                    Error::new(
                        Code::Switch,
                        format!("No account named {name} to switch to"),
                    )
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
            Code::Usage,
            format!("Run atlas {command} as yourself, not with sudo."),
        ));
    }

    Ok(())
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
    fn a_rollback_a_command_finished_is_settled_in_the_task_that_claimed_the_plan() {
        let dir = tempfile::tempdir().unwrap();
        let client = Client::new(Config {
            helper_uid: 60001,
            socket_path: dir.path().join("helper.sock"),
            identity_probe: dir.path().join("probe"),
            input_socket: dir.path().join("input.sock"),
            output_dir: dir.path().join("out"),
        })
        .unwrap();
        let (task, session, plan) = ("5be0d1a2c3f4", "3f9a1c07b2e4", atlas::files::token());
        let tasks = client.output_dir().unwrap().join("tasks");
        std::fs::create_dir(&tasks).unwrap();
        let record = tasks.join(format!("{task}.json"));
        let written = json!({"version": 1, "key": task, "session": session, "name": "probe",
            "started": 1, "updated": 1,
            "owned": [{"kind": "plan", "id": plan, "at": 1, "state": "made"}]});
        std::fs::write(&record, written.to_string()).unwrap();
        let state = || {
            let read: Value = serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
            read["owned"][0]["state"].clone()
        };
        let command = |args: &[&str]| parse(args).unwrap().command.unwrap();
        let rolled_back = Ok(json!({"ok": true, "planId": plan, "phase": "rolledBack",
            "keptTargets": [], "retainedDirectories": []}));

        let preview = command(&["environment", "rollback", &plan]);
        note_outcome(&client, None, Policy::of(&preview).kind, None, &rolled_back);
        assert_eq!(state(), "made", "a preview undoes nothing");

        let undo = command(&["environment", "rollback", &plan, "--apply"]);
        note_outcome(&client, None, Policy::of(&undo).kind, None, &rolled_back);
        assert_eq!(state(), "gone");
    }

    #[test]
    fn a_task_name_that_is_not_plain_refuses_only_commands_that_make_something() {
        use atlas::host_session::Host;

        let host = HostSession::new(Host::Codex, "tasks").unwrap();
        let task = |args: &[&str], name: &str| {
            let command = parse(args).unwrap().command.unwrap();
            task_of(
                Policy::of(&command).makes,
                Some(&host),
                Some(OsStr::new(name)),
            )
        };

        for making in [
            &["open", "--app", "TextEdit"][..],
            &["run", "--", "ls"],
            &["environment", "manifest.toml"],
            &["environment", "manifest.toml", "--apply"],
            &["environment", "apply", "plan"],
            &["environment", "rollback", "plan", "--apply"],
            &["environment", "profile", "profile.toml"],
            &["environment", "run", "--", "ls"],
            &["cleanup"],
            &["cleanup", "--apply"],
        ] {
            assert_eq!(
                task(making, "not plain").unwrap_err().code,
                "usage",
                "{making:?}"
            );
            let named = task(making, "probe").unwrap().unwrap();
            assert_eq!(named.name().map(|name| name.as_str()), Some("probe"));
        }

        for other in [
            &["status"][..],
            &["activity"],
            &["stop"],
            &["resume"],
            &["restart"],
            &["apps"],
            &["click", "1", "2"],
            &["close", "--app", "TextEdit"],
            &["environment", "list"],
        ] {
            assert_eq!(task(other, "not plain").unwrap(), None, "{other:?}");
            assert!(task(other, "probe").unwrap().is_some(), "{other:?}");
        }

        let open = parse(&["open", "--app", "TextEdit"])
            .unwrap()
            .command
            .unwrap();
        assert_eq!(
            task_of(Policy::of(&open).makes, None, Some(OsStr::new("not plain"))).unwrap(),
            None,
            "without a host session there is no task to name"
        );
    }

    /// The agents `flags` pick for a connect.
    fn picked(flags: &[&str]) -> Vec<atlas::agents::Agent> {
        let args: Vec<&str> = ["agents", "install"].iter().chain(flags).copied().collect();
        match parse(&args).unwrap().command.unwrap() {
            Command::Agents {
                action: Some(AgentsCommand::Install { picks }),
            } => picks.agents(),
            _ => panic!("a connect"),
        }
    }

    #[test]
    fn pi_and_opencode_can_be_selected_independently_or_together() {
        use atlas::agents::Agent;
        assert_eq!(picked(&["--pi"]), [Agent::Pi]);
        assert_eq!(picked(&["--opencode"]), [Agent::Opencode]);
        assert_eq!(
            picked(&["--pi", "--opencode"]),
            [Agent::Opencode, Agent::Pi]
        );
        for verb in ["install", "remove"] {
            assert!(parse(&["agents", verb, "--pi", "--opencode"]).is_ok());
        }
    }

    #[test]
    fn cursor_has_its_own_selector_and_internal_hooks_stay_out_of_help() {
        assert_eq!(picked(&["--cursor"]), [atlas::agents::Agent::Cursor]);
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
            Policy::of(&command).doing.map(|doing| {
                let mut row = atlas::activity_log::Row {
                    ok: Some(true),
                    action_count: Some(1),
                    ..Default::default()
                };
                doing.fill(&mut row, true, None);
                atlas::doing::says(&row)
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
            let mut row = atlas::activity_log::Row::default();
            Policy::of(&command)
                .doing
                .unwrap()
                .fill(&mut row, true, None);
            row.app.map(|app| json!(app))
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

    #[test]
    fn typed_text_names_the_field_it_was_sent_to_once_the_helper_checked_it() {
        let said = |args: &[&str], role: Option<&str>| {
            let command = parse(args).unwrap().command.unwrap();
            let mut row = atlas::activity_log::Row {
                ok: Some(true),
                ..Default::default()
            };
            Policy::of(&command)
                .doing
                .unwrap()
                .fill(&mut row, true, role);
            atlas::doing::says(&row)
        };
        let typed = ["type", "--app", "Notes", "--field", "Email", "secret"];

        assert_eq!(
            said(&typed, Some("AXTextField")),
            "typed into Email in Notes"
        );
        assert_eq!(said(&typed, None), "typed in Notes");
        assert_eq!(
            said(&["type", "--app", "Notes", "secret"], Some("AXTextField")),
            "typed in Notes"
        );
        assert_eq!(
            said(
                &["key", "--app", "Notes", "--field", "Email", "tab"],
                Some("AXTextField")
            ),
            "pressed Tab in Notes"
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
        client.doing = Policy::of(&command).doing;
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

    #[tokio::test]
    async fn atlas_run_lets_others_act_while_it_runs() {
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
        let command = parse(&["run", "--", "tool"]).unwrap().command.unwrap();
        client.doing = Policy::of(&command).doing;
        let other = atlas::control::Controller::at(60001, &dir.path().join("state"));
        let listener = UnixListener::bind(&client.transport.path).unwrap();
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

            // The run lets go once it has sent its request; a child another
            // test is starting may hold the lock's file a moment past its
            // release, until it runs its own program.
            let mut beside = None;
            for _ in 0..50 {
                tokio::time::sleep(Duration::from_millis(20)).await;
                beside = other.acquire_beside_commands().ok();
                if beside.is_some() {
                    break;
                }
            }
            assert!(beside.is_some(), "another agent acts while it runs");
            drop(beside);
            assert_eq!(other.acquire(true).unwrap_err().code, "busy");

            peer.get_mut()
                .write_all(
                    format!(
                        "{}\n",
                        json!({"ok": true, "exitCode": 0, "timedOut": false})
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        };
        let client = || Ok(client.clone());
        let running = carry_out(Path::new("unused.toml"), command, &client, None);
        let (outcome, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(running, helper)
        })
        .await
        .unwrap();
        assert_eq!(outcome.unwrap()["exitCode"], 0);
        assert!(other.pending().unwrap().is_empty());
    }

    #[test]
    fn a_stop_refuses_what_changes_the_helper_and_lets_looking_and_recovery_through() {
        let changes = |args: &[&str]| {
            let command = parse(args).unwrap().command.unwrap();
            Policy::of(&command).changes_helper
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
            &["job", "start", "--", "/bin/sleep", "9"],
            &["job", "stop", "7c2e91a04b3f"],
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
            &["environment", "list", "--all"],
            &["environment", "profile", "profile.toml"],
            &["update", "--check"],
            &["setup"],
            &["job", "list", "--all"],
            &["job", "log", "7c2e91a04b3f"],
            &["collect", "atlas-work/report.pdf"],
        ] {
            assert!(!changes(looking), "{looking:?}");
        }
    }

    #[test]
    fn servers_helper_commands_and_setup_carry_no_host_session() {
        let attributed = |args: &[&str]| {
            Policy::of(&parse(args).unwrap().command.unwrap()).labelled(Actor::Agent)
        };

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
        let kind = |args: &[&str]| Policy::of(&parse(args).unwrap().command.unwrap()).kind;

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
            &["environment", "list"],
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
            assert!(Policy::of(&command).labelled(Actor::Agent), "{args:?}");
            assert!(!Policy::of(&command).labelled(Actor::Viewer), "{args:?}");
        }
    }

    #[test]
    fn only_commands_that_take_control_wait_and_listen_for_signals() {
        let waits = |args: &[&str]| Policy::of(&parse(args).unwrap().command.unwrap()).controls;

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
            &["job", "start", "--", "/bin/sleep", "9"],
            &["job", "stop", "7c2e91a04b3f"],
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
            &["job", "list"],
            &["job", "log", "7c2e91a04b3f"],
            &["collect", "a.bin"],
        ] {
            assert!(!waits(at_once), "{at_once:?}");
        }
    }

    #[test]
    fn job_and_collect_rows_name_no_arguments_or_files() {
        let fields = |args: &[&str]| {
            let command = parse(args).unwrap().command.unwrap();
            let mut row = atlas::activity_log::Row::default();
            Policy::of(&command)
                .doing
                .unwrap()
                .fill(&mut row, true, None);
            serde_json::to_value(row).unwrap()
        };
        for (args, verb) in [
            (
                &[
                    "job",
                    "start",
                    "--name",
                    "web",
                    "--",
                    "/usr/bin/nc",
                    "-lk",
                    "8765",
                ][..],
                "job-start",
            ),
            (&["job", "stop", "7c2e91a04b3f"], "job-stop"),
            (&["collect", "atlas-work/secret-report.pdf"], "collect"),
        ] {
            let fields = fields(args);
            assert_eq!(fields["verb"], verb);
            let text = fields.to_string();
            for word in ["web", "nc", "8765", "secret-report", "7c2e91a04b3f"] {
                assert!(!text.contains(word), "{args:?}: {text}");
            }
        }
        let list = parse(&["job", "list"]).unwrap().command.unwrap();
        assert!(Policy::of(&list).doing.is_none());
    }

    #[test]
    fn starting_jobs_and_collecting_are_refused_for_a_task_name_that_is_not_plain() {
        for args in [
            &["job", "start", "--", "/bin/sleep", "9"][..],
            &["collect", "a.bin"],
        ] {
            let command = parse(args).unwrap().command.unwrap();
            assert!(Policy::of(&command).makes, "{args:?}");
        }
        let list = parse(&["job", "list"]).unwrap().command.unwrap();
        assert!(!Policy::of(&list).makes);
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
            assert!(Policy::of(&command).waits(Actor::Agent), "{args:?}");
            assert!(!Policy::of(&command).waits(Actor::Viewer), "{args:?}");
        }
        let looking = parse(&["screenshot"]).unwrap().command.unwrap();
        assert!(!Policy::of(&looking).waits(Actor::Agent));
    }

    #[test]
    fn setup_commands_refuse_to_run_as_root() {
        let error = refuse_root("update", true).unwrap_err();
        assert_eq!(error.error, "Run atlas update as yourself, not with sudo.");
        assert!(refuse_root("init", false).is_ok());
    }
}
