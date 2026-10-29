//! What each command is to the rest of Atlas, decided in one place: what
//! it does in the helper as the activity log keeps it, whose work it is,
//! whether it makes, changes or takes control of anything there, how a stop
//! ends it, and who reads its result. A new command is described here.
use super::cli::{AgentsCommand, ButtonArg, Command, EnvironmentCommand, JobCommand};
use atlas::{
    client::Actor,
    doing::Doing,
    host_session::store::Kind,
    ui::{render::View, text},
};

/// What a command is to the rest of Atlas.
pub(super) struct Policy {
    /// What it does in the helper, as the activity log keeps it; none for a
    /// command that acts on nothing there.
    pub(super) doing: Option<Doing>,
    /// What its host session's record keeps of it.
    pub(super) kind: Kind,
    /// Whether it is its host session's work.
    attributed: bool,
    /// Whether it makes something its task's record claims, or cleans it up.
    pub(super) makes: bool,
    /// Whether it changes the helper, so an agent's is refused while the
    /// person has stopped agents.
    pub(super) changes_helper: bool,
    /// Whether it takes control of the helper.
    pub(super) controls: bool,
    /// Whether another Atlas process reads its result.
    pub(super) machine_only: bool,
    /// What Ctrl-C at a terminal says when it stops it; none for a command
    /// a signal stops another way.
    pub(super) interrupted: Option<&'static str>,
    /// Whether it runs agent tools, which a stop ends with it.
    pub(super) runs_agents: bool,
    /// How its result reads for a person.
    pub(super) view: View,
}

impl Policy {
    /// The policy of `command`.
    pub(super) fn of(command: &Command) -> Self {
        let doing = doing(command);
        let changes_helper = changes_helper(command, doing.is_some());
        Self {
            kind: kind(command),
            attributed: attributed(command),
            makes: makes(command),
            changes_helper,
            controls: controls(command),
            machine_only: machine_only(command),
            interrupted: interrupted(command),
            runs_agents: runs_agents(command),
            view: view(command),
            doing,
        }
    }

    /// Whether the command, run by `actor`, is labelled with the host
    /// session it comes from: an agent's own work only. What the person
    /// does from the app, the menu bar or a viewer never is.
    pub(super) fn labelled(&self, actor: Actor) -> bool {
        actor == Actor::Agent && self.attributed
    }

    /// Whether the command, run by `actor`, waits for another controller to
    /// let go of the helper: only an agent's command that takes control
    /// does. What the person does from the app answers at once, and so do
    /// servers, which take no control of their own: a wait blocks the
    /// thread it runs on.
    pub(super) fn waits(&self, actor: Actor) -> bool {
        self.controls && actor == Actor::Agent
    }
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
        Command::Key { keys, target, .. } => Doing::keys(keys).app(target.app.as_deref()),
        Command::Open { app, targets, .. } => Doing::open(app.as_deref(), targets),
        Command::Close { app, .. } => Doing::new("close").app(app.as_deref()),
        Command::Cleanup { apply: true, .. } => Doing::new("cleanup"),
        Command::InstallApp { path } => Doing::new("install").app(path.to_str()),
        Command::Run { .. }
        | Command::Environment {
            command: Some(EnvironmentCommand::Run { .. }),
            ..
        } => Doing::new("run"),
        Command::Job {
            command: JobCommand::Start { .. },
        } => Doing::new("job-start"),
        Command::Job {
            command: JobCommand::Stop { .. },
        } => Doing::new("job-stop"),
        Command::Collect { .. } => Doing::new("collect"),
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
            | Command::JobLaunch { .. }
            | Command::InputService { .. }
            | Command::EnvironmentWorker { .. }
            | Command::SessionInfo
            | Command::PermissionsInfo
            | Command::WindowsInfo { .. }
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

/// Whether a command makes something its task's record claims (a copy, an
/// app opened by name, an environment plan or a run in the helper) or
/// cleans it up: told of a task by a name that is not one, a cleanup would
/// cover the whole session instead. Only these are refused for an
/// ATLAS_TASK that names no task; looking, Stop and recovery never depend
/// on it.
fn makes(command: &Command) -> bool {
    match command {
        Command::Open { .. }
        | Command::Run { .. }
        | Command::InstallApp { .. }
        | Command::Cleanup { .. }
        | Command::Collect { .. }
        | Command::Job {
            command: JobCommand::Start { .. } | JobCommand::Stop { .. },
        } => true,
        Command::Environment { command, .. } => matches!(
            command,
            None | Some(
                EnvironmentCommand::Apply { .. }
                    | EnvironmentCommand::Rollback { .. }
                    | EnvironmentCommand::Profile { .. }
                    | EnvironmentCommand::Run { .. }
            )
        ),
        _ => false,
    }
}

/// Whether a command changes the helper, so an agent's is refused while
/// the person has stopped agents. Looking, reading and recovery are not;
/// any other command does when it is `recorded` as doing something there.
fn changes_helper(command: &Command, recorded: bool) -> bool {
    match command {
        // Collecting only reads in the helper, and works during a Stop.
        Command::Screenshot { .. } | Command::Collect { .. } => false,
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
        // Its preview only reads, and works during a Stop.
        Command::Cleanup { apply, .. } => *apply,
        Command::Environment { apply, command, .. } => match command {
            None => *apply,
            Some(EnvironmentCommand::Run { .. } | EnvironmentCommand::Apply { .. }) => true,
            Some(EnvironmentCommand::Rollback { apply, .. }) => *apply,
            Some(EnvironmentCommand::Profile { apply, .. }) => *apply,
            Some(_) => false,
        },
        _ => recorded,
    }
}

/// Whether an agent's command may take control of the helper: it waits for
/// another command to finish first. A logout that asks first is the
/// person's at a terminal, and answers at once. Whoever runs one, a signal
/// stops it cleanly: nothing more starts, and the step under way settles
/// or is cut. `logout --yes` run at a terminal is the exception: it keeps
/// the stop every logout at a terminal has, which ends it at once.
fn controls(command: &Command) -> bool {
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
            | Command::Cleanup { apply: true, .. }
            | Command::InstallApp { .. }
            | Command::Run { .. }
            | Command::Job {
                command: JobCommand::Start { .. } | JobCommand::Stop { .. },
            }
            | Command::Restart
            | Command::StartInput
            | Command::Install
            | Command::Readiness { .. }
            | Command::Environment { .. }
            | Command::VerifyNative { .. }
            | Command::Logout { yes: true, .. }
    )
}

/// Commands Atlas runs itself (in the helper, or as a step of another
/// command), whose output another Atlas process reads.
fn machine_only(command: &Command) -> bool {
    matches!(
        command,
        Command::EnvironmentWorker { .. }
            | Command::SessionInfo
            | Command::PermissionsInfo
            | Command::WindowsInfo { .. }
            | Command::ElementsInfo { .. }
            | Command::HelperServe { .. }
            | Command::JobLaunch { .. }
            | Command::InputService { .. }
            | Command::SetupHelper { .. }
            | Command::UpdateFinish
            | Command::Onboarding { .. }
    )
}

/// What Ctrl-C at a terminal says when it stops a command that asks
/// questions or waits for the person, saying what that means for each.
fn interrupted(command: &Command) -> Option<&'static str> {
    match command {
        Command::Init => Some("Stopped. Run atlas init again to continue."),
        Command::Switch { .. }
        | Command::Uninstall { .. }
        | Command::Logout { .. }
        | Command::Agents { .. } => Some("Stopped."),
        Command::Update { check: false, .. } => Some("Stopped. Run atlas update again to finish."),
        _ => None,
    }
}

/// Whether a command runs agent tools (each in a process group of its
/// own), which are stopped whenever it is, a person watching or not.
fn runs_agents(command: &Command) -> bool {
    matches!(
        command,
        Command::Init | Command::Uninstall { .. } | Command::Agents { .. } | Command::UpdateFinish
    )
}

/// How a command's result reads for a person.
fn view(command: &Command) -> View {
    let at = |x: f64, y: f64| format!("{}, {}", text::number(x), text::number(y));

    match command {
        Command::Doctor {
            login_timing: true, ..
        } => View::LoginTiming,
        Command::Doctor { verbose, .. } => View::Doctor { verbose: *verbose },
        Command::Status => View::Status,
        Command::Activity { .. } => View::Activity,
        Command::Config => View::Config,
        Command::Screenshot { .. } => View::Capture,
        Command::Apps { .. } => View::Apps,
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
        Command::Key { keys, .. } => View::Action(format!("Pressed {}", keys.join(" "))),
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
        Command::Cleanup { .. } => View::Cleanup,
        Command::Job {
            command: JobCommand::Start { .. } | JobCommand::Stop { .. },
        } => View::Job,
        Command::Job {
            command: JobCommand::List { .. },
        } => View::Jobs,
        Command::Job {
            command: JobCommand::Log { .. },
        } => View::JobLog,
        Command::Collect { .. } => View::Collect,
        Command::InstallApp { .. } => View::InstallApp,
        Command::Environment {
            command: Some(EnvironmentCommand::Discover { .. }),
            ..
        } => View::Discover,
        Command::Environment {
            command: Some(EnvironmentCommand::List { .. }),
            ..
        } => View::Plans,
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
        Command::Stop => View::Action("Stopped the agent".into()),
        Command::Resume => View::Action("Resumed the agent".into()),
        Command::Logout { .. } => View::Logout,
        Command::StartInput => View::StartInput,
        Command::Install => View::Install,
        Command::Agents {
            action: Some(AgentsCommand::Install { .. } | AgentsCommand::Remove { .. }),
        } => View::AgentsChange,
        Command::Agents {
            action: Some(AgentsCommand::Project { .. } | AgentsCommand::Forget { .. }),
        } => View::AgentsProject,
        Command::Agents { .. } => View::Agents,
        Command::Switch { .. } => View::Switch,
        Command::Update { check: true, .. } => View::Generic,
        Command::Update { .. } => View::Update,
        Command::Uninstall { .. } => View::Uninstall,
        Command::Menubar { login: Some(_), .. } => View::Login,
        Command::Menubar { .. } => View::Menubar,
        Command::Monitor { .. } => View::Monitor,
        Command::Init | Command::Watch { .. } | Command::Mcp { .. } => View::Quiet,
        _ => View::Generic,
    }
}
