import Foundation

/// What `atlas onboarding status` reports: everything the onboarding window
/// needs to choose its step, read fresh each time.
public struct OnboardingFacts: Equatable, Sendable {
    public enum Role: String, Sendable { case controller, helper }

    public struct Candidate: Equatable, Sendable {
        public let name: String
        public let realName: String?
        public var title: String { realName ?? name }
    }

    public struct Helper: Equatable, Sendable {
        public var exists = false
        public var name = ""
        public var realName: String?
        public var administrator: Bool?
        public var leftoverSession = false
        /// This account's request names this helper.
        public var paired = false
        public var signedIn = false
        public var answering = false
        public var onConsole = false
        public var hidden = false
        public var screenCapture = false
        public var accessibility = false
        public var title: String { realName ?? name }
    }

    public struct Asker: Equatable, Sendable {
        public var uid = 0
        public var name = ""
        public var realName: String?
        public var installed = false
        public var running = false
        public var screenCapture = false
        public var accessibility = false
        public var title: String { realName ?? name }
    }

    public var role: Role = .controller
    public var myName = ""
    public var appProblem: String?
    public var configured = false
    public var candidates: [Candidate] = []
    public var helper = Helper()
    public var finished = false
    /// Open at Login was decided once already (so the person's choice stands).
    public var loginDecided = false
    /// In the helper: the account that asked, and this account's state.
    public var asker = Asker()

    public init() {}

    public init(_ reply: [String: Any]) {
        role = Role(rawValue: reply["role"] as? String ?? "") ?? .controller
        let me = reply["me"] as? [String: Any] ?? [:]
        myName = (me["realName"] as? String) ?? (me["name"] as? String ?? "")
        let app = reply["app"] as? [String: Any] ?? [:]
        if app["installed"] as? Bool == false {
            appProblem = app["error"] as? String ?? "Atlas.app must be in Applications"
        } else if app["signed"] as? Bool == false {
            appProblem = "This Atlas.app is not signed by its developer; install it again"
        }
        configured = reply["configured"] as? Bool ?? false
        candidates = (reply["candidates"] as? [[String: Any]] ?? []).compactMap {
            guard let name = $0["name"] as? String else { return nil }
            return Candidate(name: name, realName: $0["realName"] as? String)
        }
        finished = reply["finished"] as? Bool ?? false
        loginDecided = reply["menubarLogin"] as? Bool ?? false
        let helper = reply["helper"] as? [String: Any] ?? [:]
        switch role {
        case .controller:
            let permissions = helper["permissions"] as? [String: Any] ?? [:]
            self.helper = Helper(
                exists: helper["exists"] as? Bool ?? false,
                name: helper["name"] as? String ?? "",
                realName: helper["realName"] as? String,
                administrator: helper["administrator"] as? Bool,
                leftoverSession: helper["leftoverSession"] as? Bool ?? false,
                paired: helper["paired"] as? Bool ?? false,
                signedIn: helper["signedIn"] as? Bool ?? false,
                answering: helper["answering"] as? Bool ?? false,
                onConsole: helper["onConsole"] as? Bool ?? false,
                hidden: helper["hidden"] as? Bool ?? false,
                screenCapture: permissions["screenCapture"] as? Bool ?? false,
                accessibility: permissions["accessibility"] as? Bool ?? false)
        case .helper:
            let controller = helper["controller"] as? [String: Any] ?? [:]
            let permissions = helper["permissions"] as? [String: Any] ?? [:]
            asker = Asker(
                uid: controller["uid"] as? Int ?? 0,
                name: controller["name"] as? String ?? "",
                realName: controller["realName"] as? String,
                installed: helper["installed"] as? Bool ?? false,
                running: helper["running"] as? Bool ?? false,
                screenCapture: permissions["screenCapture"] as? Bool ?? false,
                accessibility: permissions["accessibility"] as? Bool ?? false)
        }
    }
}

/// The onboarding's screens, in the order the person meets them.
public enum OnboardingStep: Equatable, Sendable {
    // In the person's account.
    case welcome
    case chooseHelper
    case createAccount
    case switchToHelper
    case finishing
    case agents
    case done
    // In the helper's account.
    case allow
    case permissions
    case helperDone
    // Something only the person can fix first.
    case blocked(String)

    /// Where the step sits among the dots at the bottom, and how many.
    public var dots: (index: Int, count: Int)? {
        switch self {
        case .welcome: return (0, 6)
        case .chooseHelper, .createAccount: return (1, 6)
        case .switchToHelper: return (2, 6)
        case .finishing: return (3, 6)
        case .agents: return (4, 6)
        case .done: return (5, 6)
        case .allow: return (0, 3)
        case .permissions: return (1, 3)
        case .helperDone: return (2, 3)
        case .blocked: return nil
        }
    }
}

/// What the person has chosen in this window, which the facts cannot say.
public struct OnboardingChoices: Equatable, Sendable {
    /// Past the welcome screen.
    public var started = false
    /// On the account form rather than the list.
    public var creating = false
    /// The agents screen is behind them.
    public var agentsDone = false

    public init() {}
}

/// The screen to show, decided from the facts and the person's choices
/// alone, so any state, including a window opened half-way, lands right.
public func onboardingStep(_ facts: OnboardingFacts, _ choices: OnboardingChoices) -> OnboardingStep {
    if let problem = facts.appProblem { return .blocked(problem) }
    if facts.role == .helper {
        let asker = facts.asker
        // Set up from a terminal (`atlas init`): nothing to allow, but the
        // permissions are still checked.
        if !asker.installed || !asker.running { return asker.name.isEmpty ? .permissions : .allow }
        if !asker.accessibility || !asker.screenCapture { return .permissions }
        return .helperDone
    }
    if !facts.configured || !facts.helper.exists {
        if !choices.started && !facts.configured { return .welcome }
        return choices.creating ? .createAccount : .chooseHelper
    }
    let helper = facts.helper
    if helper.administrator == true {
        return .blocked(
            "\(helper.title) administers this Mac. Make it a standard account in System Settings › Users & Groups, or choose another account.")
    }
    if helper.leftoverSession {
        return .blocked(
            "macOS still runs a session of a deleted account with \(helper.title)'s user ID. Restart the Mac, then open Atlas again.")
    }
    if facts.finished {
        return choices.agentsDone ? .done : .agents
    }
    // Chosen, but the helper was never asked (cut short): choose again.
    if !helper.paired && !helper.answering {
        return choices.creating ? .createAccount : .chooseHelper
    }
    let allowed = helper.accessibility && helper.screenCapture
    if helper.answering && allowed && helper.hidden { return .finishing }
    return .switchToHelper
}

/// The three rows on the switch screen: done, current or waiting.
public enum RowState: String, Sendable { case waiting, current, done }

public func switchRows(_ helper: OnboardingFacts.Helper) -> [RowState] {
    let allowed = helper.accessibility && helper.screenCapture
    if !helper.signedIn { return [.current, .waiting, .waiting] }
    if !helper.answering { return [.done, .current, .waiting] }
    if !allowed { return [.done, .done, .current] }
    return [.done, .done, .done]
}
