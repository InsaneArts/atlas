//! What each command is to the rest of Atlas, decided in one place: what it
//! does in the helper as the activity log keeps it, how it uses the helper,
//! who runs it, how a stop ends it and how its result reads. A new command
//! must choose a row.
use super::cli::{self, AgentsCommand, Command, EnvironmentCommand, JobCommand};
use atlas_cli::ui::{render::View, text};
use atlas_client::{client::Actor, doing::Doing, host_session::store::Kind};

/// What a command is to the rest of Atlas.
pub(super) struct Policy {
    /// What it does in the helper; none for a command that acts on nothing there.
    pub(super) doing: Option<Doing>,
    /// What its host session's record keeps of it.
    pub(super) kind: Kind,
    work: Work,
    audience: Audience,
    stop: Stop,
    /// How its result reads for a person.
    pub(super) view: View,
}

/// How a command uses the helper. Collecting claims local output without
/// changing the helper; inspecting takes control without changing it;
/// service changes acquire their own control.
enum Work {
    Read,
    Inspect,
    Change,
    Make,
    Collect,
    /// A plan's preview, or with `true` its application.
    Environment(bool),
    ServiceChange,
}

/// Whose work a command is and who reads its result.
enum Audience {
    /// Its host session's work.
    Agent,
    /// The person's, or a server's: no host session's work.
    Local,
    /// Another Atlas process reads its result.
    Machine,
    /// Another Atlas process reads the result of a host session's work.
    AgentMachine,
}

/// How a signal ends a command.
enum Stop {
    Ordinary,
    /// A person is asked things, and Ctrl-C says this.
    Prompt(&'static str),
    /// It runs agent tools, which a stop ends with it; Ctrl-C says this.
    Tools(Option<&'static str>),
}

/// A point as a person reads it.
fn at(x: f64, y: f64) -> String {
    format!("{}, {}", text::number(x), text::number(y))
}

impl Policy {
    pub(super) fn of(command: &Command) -> Self {
        use Audience::{Agent, AgentMachine, Local, Machine};
        use Stop::{Ordinary, Prompt, Tools};
        use Work::{Change, Collect, Inspect, Make, Read, ServiceChange};

        match command {
            Command::Screenshot(_) => Self::helper(
                Read,
                View::Capture,
                Some(Doing::new("screenshot")),
                Kind::Screenshot,
            ),
            Command::Click(click) => Self::work(
                Change,
                View::Action(match (click.x, click.y) {
                    (Some(x), Some(y)) => format!("Clicked at {}", at(x, y)),
                    _ => "Clicked".into(),
                }),
                Doing::click(click.button.into(), click.count)
                    .app(click.target.app.as_deref())
                    .label(click.name.as_deref()),
            ),
            Command::Element(element) => {
                let (verb, done) = element.action.words();
                Self::work(
                    Change,
                    View::Action(done.into()),
                    Doing::new(verb).app(element.target.app.as_deref()),
                )
            }
            Command::Move { x, y } => Self::work(
                Change,
                View::Action(format!("Moved the pointer to {}", at(*x, *y))),
                Doing::new("move"),
            ),
            Command::Drag(drag) => Self::work(
                Change,
                View::Action(format!(
                    "Dragged from {} to {}",
                    at(drag.from_x, drag.from_y),
                    at(drag.to_x, drag.to_y)
                )),
                Doing::new("drag").app(drag.target.app.as_deref()),
            ),
            Command::Scroll(scroll) => Self::work(
                Change,
                View::Action(format!("Scrolled at {}", at(scroll.x, scroll.y))),
                Doing::new("scroll").app(scroll.target.app.as_deref()),
            ),
            Command::Type(typed) => Self::work(
                Change,
                View::Type,
                Doing::new("type")
                    .app(typed.target.app.as_deref())
                    .field(typed.field.as_deref()),
            ),
            Command::Key(keys) => Self::work(
                Change,
                View::Action(format!("Pressed {}", keys.keys.join(" "))),
                Doing::keys(&keys.keys).app(keys.target.app.as_deref()),
            ),
            Command::Open(open) => Self::helper(
                Make,
                View::Open(
                    open.app
                        .iter()
                        .chain(&open.targets)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                Some(Doing::open(open.app.as_deref(), &open.targets)),
                Kind::Open,
            ),
            Command::Close { app, .. } => Self::helper(
                Change,
                View::Close,
                Some(Doing::new("close").app(app.as_deref())),
                Kind::Close,
            ),
            Command::Cleanup(cleanup) if cleanup.apply => {
                Self::work(Make, View::Cleanup, Doing::new("cleanup"))
            }
            Command::Cleanup(_) => Self::helper(Collect, View::Cleanup, None, Kind::Other),
            Command::InstallApp { path } => Self::work(
                Make,
                View::InstallApp,
                Doing::new("install").app(path.to_str()),
            ),
            Command::Run(_) => Self::work(Make, View::Run, Doing::new("run")),
            Command::Job { command } => Self::job(command),
            Command::Collect { .. } => Self::work(Collect, View::Collect, Doing::new("collect")),
            Command::Environment(environment) => Self::environment(environment),
            Command::Input { .. } => Self::helper(
                Change,
                View::Action("Performed the actions".into()),
                None,
                Kind::Other,
            ),
            Command::Logout { yes, .. } => Self {
                stop: Prompt("Stopped."),
                ..Self::helper(
                    if *yes { Change } else { ServiceChange },
                    View::Logout,
                    None,
                    Kind::Other,
                )
            },
            Command::VerifyNative { .. } => Self::helper(Change, View::Generic, None, Kind::Other),
            Command::Install => Self::helper(Change, View::Install, None, Kind::Other),
            Command::Restart => Self::helper(Inspect, View::Restart, None, Kind::Other),
            Command::StartInput => Self::helper(Inspect, View::StartInput, None, Kind::Other),
            Command::Readiness { .. } => Self::helper(Inspect, View::Readiness, None, Kind::Other),
            Command::Setup(setup) if setup.apply => Self {
                work: ServiceChange,
                ..Self::passive(Agent, Ordinary, View::Generic)
            },
            Command::Setup(_) => Self::local(View::SetupPreview),
            Command::Doctor(doctor) if doctor.login_timing => Self::read(View::LoginTiming),
            Command::Doctor(doctor) => Self::read(View::Doctor {
                verbose: doctor.verbose,
            }),
            Command::Status => Self::read(View::Status),
            Command::Activity(_) => Self::read(View::Activity),
            Command::Config => Self::read(View::Config),
            Command::Apps { .. } => Self::read(View::Apps),
            Command::Windows { .. } => Self::read(View::Windows),
            Command::Elements(_) => Self::read(View::Elements),
            Command::Stop => Self::read(View::Action("Stopped the agent".into())),
            Command::Resume => Self::read(View::Action("Resumed the agent".into())),
            Command::Wait(wait) => Self::read(View::Wait {
                gone: wait.gone || wait.state == Some(cli::ElementState::Gone),
            }),
            Command::AgentGuide(_) => Self::read(View::Generic),
            Command::Mcp(_) | Command::Watch { .. } => Self::local(View::Quiet),
            Command::Menubar(menubar) if menubar.login.is_some() => Self::local(View::Login),
            Command::Menubar(_) => Self::local(View::Menubar),
            Command::Monitor { terminal: true, .. } => Self::local(View::Quiet),
            Command::Monitor { .. } => Self::local(View::Monitor),
            Command::HelperServe(_)
            | Command::JobLaunch { .. }
            | Command::InputService(_)
            | Command::EnvironmentWorker { .. }
            | Command::SessionInfo
            | Command::SessionProofServe
            | Command::PermissionsInfo
            | Command::ElementsInfo { .. }
            | Command::SetupHelper(_)
            | Command::Onboarding { .. } => Self::passive(Machine, Ordinary, View::Generic),
            Command::Update(update) if update.check => Self::local(View::Generic),
            Command::Update(_) => Self {
                work: ServiceChange,
                ..Self::passive(
                    Agent,
                    Prompt("Stopped. Run atlas update again to finish."),
                    View::Update,
                )
            },
            Command::UpdateFinish => Self::passive(AgentMachine, Tools(None), View::Generic),
            Command::Switch(_) => Self::passive(Local, Prompt("Stopped."), View::Switch),
            Command::Init => Self::passive(
                Local,
                Tools(Some("Stopped. Run atlas init again to continue.")),
                View::Quiet,
            ),
            Command::Uninstall(_) => Self::passive(Local, Tools(Some("Stopped.")), View::Uninstall),
            Command::Agents { action } => Self::passive(
                Agent,
                Tools(Some("Stopped.")),
                match action {
                    Some(AgentsCommand::Install { .. } | AgentsCommand::Remove { .. }) => {
                        View::AgentsChange
                    }
                    Some(AgentsCommand::Project { .. } | AgentsCommand::Forget { .. }) => {
                        View::AgentsProject
                    }
                    None
                    | Some(AgentsCommand::Status { .. } | AgentsCommand::CursorHook { .. }) => {
                        View::Agents
                    }
                },
            ),
        }
    }

    fn job(command: &JobCommand) -> Self {
        match command {
            JobCommand::Start { .. } => Self::work(Work::Make, View::Job, Doing::new("job-start")),
            JobCommand::Stop { .. } => Self::work(Work::Make, View::Job, Doing::new("job-stop")),
            JobCommand::List { .. } => Self::read(View::Jobs),
            JobCommand::Log { .. } => Self::read(View::JobLog),
        }
    }

    fn environment(environment: &cli::Environment) -> Self {
        let plan = |apply: bool| {
            Self::helper(
                Work::Environment(apply),
                View::Environment,
                None,
                Kind::Environment { changes: apply },
            )
        };

        match &environment.command {
            None => plan(environment.apply),
            Some(
                EnvironmentCommand::Rollback { apply, .. }
                | EnvironmentCommand::Profile { apply, .. },
            ) => plan(*apply),
            Some(EnvironmentCommand::Apply { .. }) => Self::helper(
                Work::Make,
                View::Environment,
                None,
                Kind::Environment { changes: true },
            ),
            Some(EnvironmentCommand::Run(_)) => {
                Self::work(Work::Make, View::Run, Doing::new("run"))
            }
            Some(EnvironmentCommand::Discover { .. }) => {
                Self::helper(Work::Inspect, View::Discover, None, Kind::Other)
            }
            Some(EnvironmentCommand::List { .. }) => {
                Self::helper(Work::Inspect, View::Plans, None, Kind::Other)
            }
        }
    }

    /// Attributed helper operations use ordinary signal handling and choose
    /// their work, view and recording together.
    fn helper(work: Work, view: View, doing: Option<Doing>, kind: Kind) -> Self {
        Self {
            doing,
            kind,
            work,
            audience: Audience::Agent,
            stop: Stop::Ordinary,
            view,
        }
    }

    /// A helper operation the activity log names and its session's record skips.
    fn work(work: Work, view: View, doing: Doing) -> Self {
        Self::helper(work, view, Some(doing), Kind::Other)
    }

    /// An attributed look at the helper or at Atlas: nothing is recorded.
    fn read(view: View) -> Self {
        Self::helper(Work::Read, view, None, Kind::Other)
    }

    /// Local flows and internal workers produce no helper activity or task resources.
    fn passive(audience: Audience, stop: Stop, view: View) -> Self {
        Self {
            doing: None,
            kind: Kind::Other,
            work: Work::Read,
            audience,
            stop,
            view,
        }
    }

    /// The person's own look or a server, stopped as ordinary.
    fn local(view: View) -> Self {
        Self::passive(Audience::Local, Stop::Ordinary, view)
    }

    /// Whether it makes something its task's record claims, or cleans it up.
    pub(super) fn makes(&self) -> bool {
        match self.work {
            Work::Make | Work::Collect | Work::Environment(_) => true,
            Work::Read | Work::Inspect | Work::Change | Work::ServiceChange => false,
        }
    }

    /// Whether it changes the helper, so an agent's is refused while the
    /// person has stopped agents.
    pub(super) fn changes_helper(&self) -> bool {
        match self.work {
            Work::Change | Work::Make | Work::ServiceChange => true,
            Work::Environment(apply) => apply,
            Work::Read | Work::Inspect | Work::Collect => false,
        }
    }

    /// Whether it takes control of the helper.
    pub(super) fn controls(&self) -> bool {
        match self.work {
            Work::Inspect | Work::Change | Work::Make | Work::Environment(_) => true,
            Work::Read | Work::Collect | Work::ServiceChange => false,
        }
    }

    /// Whether another Atlas process reads its result.
    pub(super) fn machine_only(&self) -> bool {
        match self.audience {
            Audience::Machine | Audience::AgentMachine => true,
            Audience::Agent | Audience::Local => false,
        }
    }

    /// Whether it runs agent tools, which a stop ends with it.
    pub(super) fn runs_agents(&self) -> bool {
        match self.stop {
            Stop::Tools(_) => true,
            Stop::Ordinary | Stop::Prompt(_) => false,
        }
    }

    /// What Ctrl-C at a terminal says when it stops the command; none for a
    /// command a signal stops another way.
    pub(super) fn interrupted(&self) -> Option<&'static str> {
        match self.stop {
            Stop::Prompt(message) => Some(message),
            Stop::Tools(message) => message,
            Stop::Ordinary => None,
        }
    }

    /// Whether the command, run by `actor`, is labelled with the host
    /// session it comes from: an agent's own work only. What the person
    /// does from the app, the menu bar or a viewer never is.
    pub(super) fn labelled(&self, actor: Actor) -> bool {
        actor == Actor::Agent
            && match self.audience {
                Audience::Agent | Audience::AgentMachine => true,
                Audience::Local | Audience::Machine => false,
            }
    }

    /// Whether the command, run by `actor`, waits for another controller to
    /// let go of the helper: only an agent's command that takes control
    /// does. What the person does from the app answers at once, and so do
    /// servers, which take no control of their own: a wait blocks the
    /// thread it runs on.
    pub(super) fn waits(&self, actor: Actor) -> bool {
        self.controls() && actor == Actor::Agent
    }
}
