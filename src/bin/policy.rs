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

// Each row gives activity and record kind; attribution, task creation, helper
// mutation, control, machine output and tool supervision; interrupt words; view.
macro_rules! policies {
    ($command:expr; $( $pattern:pat => ($doing:expr, $kind:expr;
        attributed: $attributed:expr, makes: $makes:expr, changes: $changes:expr, controls: $controls:expr, machine: $machine:expr, runs_agents: $runs:expr;
        $interrupted:expr; $view:expr) ),+ $(,)?) => {
        match $command {
            $( $pattern => Policy {
                doing: $doing, kind: $kind, attributed: $attributed, makes: $makes,
                changes_helper: $changes, controls: $controls, machine_only: $machine,
                runs_agents: $runs, interrupted: $interrupted, view: $view,
            }, )+
        }
    };
}

impl Policy {
    /// Every command chooses all policy facts here; new variants cannot inherit them.
    pub(super) fn of(command: &Command) -> Self {
        let at = |x: f64, y: f64| format!("{}, {}", text::number(x), text::number(y));
        policies! { command;
            Command::Screenshot { .. } => (
                Some(Doing::new("screenshot")), Kind::Screenshot;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Capture
            ),
            Command::Click { target, name, button, count, x, y, .. } => (
                Some(Doing::click(matches!(button, ButtonArg::Right), *count).app(target.app.as_deref()).label(name.as_deref())), Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Action(match (x, y) { (Some(x), Some(y)) => format!("Clicked at {}", at(*x, *y)), _ => "Clicked".into() })
            ),
            Command::Move { x, y } => (
                Some(Doing::new("move")), Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Action(format!("Moved the pointer to {}", at(*x, *y)))
            ),
            Command::Drag { target, from_x, from_y, to_x, to_y, .. } => (
                Some(Doing::new("drag").app(target.app.as_deref())), Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Action(format!("Dragged from {} to {}", at(*from_x, *from_y), at(*to_x, *to_y)))
            ),
            Command::Scroll { target, x, y, .. } => (
                Some(Doing::new("scroll").app(target.app.as_deref())), Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Action(format!("Scrolled at {}", at(*x, *y)))
            ),
            Command::Type { target, field, .. } => (
                Some(Doing::new("type").app(target.app.as_deref()).field(field.as_deref())), Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Type
            ),
            Command::Key { keys, target, .. } => (
                Some(Doing::keys(keys).app(target.app.as_deref())), Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Action(format!("Pressed {}", keys.join(" ")))
            ),
            Command::Open { app, targets, .. } => (
                Some(Doing::open(app.as_deref(), targets)), Kind::Open;
                attributed: true, makes: true, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Open(app.iter().chain(targets).cloned().collect::<Vec<_>>().join(", "))
            ),
            Command::Close { app, .. } => (
                Some(Doing::new("close").app(app.as_deref())), Kind::Close;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Close
            ),
            Command::Cleanup { apply, .. } => (
                apply.then(|| Doing::new("cleanup")), Kind::Other;
                attributed: true, makes: true, changes: *apply, controls: *apply, machine: false, runs_agents: false;
                None; View::Cleanup
            ),
            Command::InstallApp { path } => (
                Some(Doing::new("install").app(path.to_str())), Kind::Other;
                attributed: true, makes: true, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::InstallApp
            ),
            Command::Run { .. } | Command::Environment { command: Some(EnvironmentCommand::Run { .. }), .. } => (
                Some(Doing::new("run")), Kind::Other;
                attributed: true, makes: true, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Run
            ),
            Command::Job { command: JobCommand::Start { .. } } => (
                Some(Doing::new("job-start")), Kind::Other;
                attributed: true, makes: true, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Job
            ),
            Command::Job { command: JobCommand::Stop { .. } } => (
                Some(Doing::new("job-stop")), Kind::Other;
                attributed: true, makes: true, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Job
            ),
            Command::Job { command: JobCommand::List { .. } } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Jobs
            ),
            Command::Job { command: JobCommand::Log { .. } } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::JobLog
            ),
            Command::Collect { .. } => (
                Some(Doing::new("collect")), Kind::Other;
                attributed: true, makes: true, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Collect
            ),
            Command::Environment { apply, command: None, .. } => (
                None, Kind::Environment { changes: *apply };
                attributed: true, makes: true, changes: *apply, controls: true, machine: false, runs_agents: false;
                None; View::Environment
            ),
            Command::Environment { command: Some(EnvironmentCommand::Apply { .. }), .. } => (
                None, Kind::Environment { changes: true };
                attributed: true, makes: true, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Environment
            ),
            Command::Environment { command: Some(EnvironmentCommand::Rollback { apply, .. } | EnvironmentCommand::Profile { apply, .. }), .. } => (
                None, Kind::Environment { changes: *apply };
                attributed: true, makes: true, changes: *apply, controls: true, machine: false, runs_agents: false;
                None; View::Environment
            ),
            Command::Environment { command: Some(EnvironmentCommand::Discover { .. }), .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: true, machine: false, runs_agents: false;
                None; View::Discover
            ),
            Command::Environment { command: Some(EnvironmentCommand::List { .. }), .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: true, machine: false, runs_agents: false;
                None; View::Plans
            ),
            Command::Input { .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Action("Performed the actions".into())
            ),
            Command::Logout { yes, .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: true, controls: *yes, machine: false, runs_agents: false;
                Some("Stopped."); View::Logout
            ),
            Command::VerifyNative { .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Generic
            ),
            Command::Install => (
                None, Kind::Other;
                attributed: true, makes: false, changes: true, controls: true, machine: false, runs_agents: false;
                None; View::Install
            ),
            Command::Restart => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: true, machine: false, runs_agents: false;
                None; View::Restart
            ),
            Command::StartInput => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: true, machine: false, runs_agents: false;
                None; View::StartInput
            ),
            Command::Readiness { .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: true, machine: false, runs_agents: false;
                None; View::Readiness
            ),
            Command::Setup { apply, rollback, .. } => (
                None, Kind::Other;
                attributed: *apply || *rollback, makes: false, changes: *apply || *rollback, controls: false, machine: false, runs_agents: false;
                None; if *apply || *rollback { View::Generic } else { View::SetupPreview }
            ),
            Command::Doctor { verbose, login_timing } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; if *login_timing { View::LoginTiming } else { View::Doctor { verbose: *verbose } }
            ),
            Command::Status => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Status
            ),
            Command::Activity { .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Activity
            ),
            Command::Config => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Config
            ),
            Command::Apps { .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Apps
            ),
            Command::Windows { .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Windows
            ),
            Command::Elements { .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Elements
            ),
            Command::Stop => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Action("Stopped the agent".into())
            ),
            Command::Resume => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Action("Resumed the agent".into())
            ),
            Command::Wait { gone, .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Wait { gone: *gone }
            ),
            Command::AgentGuide { .. } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Generic
            ),
            Command::Mcp { .. } | Command::Watch { .. } => (
                None, Kind::Other;
                attributed: false, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Quiet
            ),
            Command::Menubar { login, .. } => (
                None, Kind::Other;
                attributed: false, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; if login.is_some() { View::Login } else { View::Menubar }
            ),
            Command::Monitor { .. } => (
                None, Kind::Other;
                attributed: false, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                None; View::Monitor
            ),
            Command::HelperServe { .. } | Command::JobLaunch { .. } | Command::InputService { .. } | Command::EnvironmentWorker { .. } | Command::SessionInfo | Command::PermissionsInfo | Command::ElementsInfo { .. } | Command::SetupHelper { .. } | Command::Onboarding { .. } => (
                None, Kind::Other;
                attributed: false, makes: false, changes: false, controls: false, machine: true, runs_agents: false;
                None; View::Generic
            ),
            Command::Update { check, .. } => (
                None, Kind::Other;
                attributed: !*check, makes: false, changes: !*check, controls: false, machine: false, runs_agents: false;
                if *check { None } else { Some("Stopped. Run atlas update again to finish.") }; if *check { View::Generic } else { View::Update }
            ),
            Command::UpdateFinish => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: true, runs_agents: true;
                None; View::Generic
            ),
            Command::Switch { .. } => (
                None, Kind::Other;
                attributed: false, makes: false, changes: false, controls: false, machine: false, runs_agents: false;
                Some("Stopped."); View::Switch
            ),
            Command::Init => (
                None, Kind::Other;
                attributed: false, makes: false, changes: false, controls: false, machine: false, runs_agents: true;
                Some("Stopped. Run atlas init again to continue."); View::Quiet
            ),
            Command::Uninstall { .. } => (
                None, Kind::Other;
                attributed: false, makes: false, changes: false, controls: false, machine: false, runs_agents: true;
                Some("Stopped."); View::Uninstall
            ),
            Command::Agents { action } => (
                None, Kind::Other;
                attributed: true, makes: false, changes: false, controls: false, machine: false, runs_agents: true;
                Some("Stopped."); match action { Some(AgentsCommand::Install { .. } | AgentsCommand::Remove { .. }) => View::AgentsChange, Some(AgentsCommand::Project { .. } | AgentsCommand::Forget { .. }) => View::AgentsProject, None | Some(AgentsCommand::Status { .. } | AgentsCommand::CursorHook { .. }) => View::Agents }
            ),
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
