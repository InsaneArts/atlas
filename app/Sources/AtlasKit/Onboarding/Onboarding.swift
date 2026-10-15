import Foundation

/// What `atlas onboarding status` reports: everything the onboarding window
/// needs to choose its step, read fresh each time.
public struct OnboardingFacts: Equatable, Sendable {
    public enum Role: String, Decodable, Sendable { case controller, helper }

    public struct Candidate: Decodable, Equatable, Sendable {
        public let name: String
        public let realName: String?
        public var title: String { realName ?? name }

        public init(name: String, realName: String? = nil) {
            self.name = name
            self.realName = realName
        }
    }

    /// The helper, as the person's account sees it.
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

        public init() {}
    }

    /// In the helper: the account that asked (unknown when a terminal set
    /// it up), and this account's own state.
    public struct Asker: Equatable, Sendable {
        public var uid = 0
        public var name = ""
        public var realName: String?
        public var installed = false
        public var running = false
        public var screenCapture = false
        public var accessibility = false
        public var title: String { realName ?? name }

        public init() {}
    }

    public var role: Role = .controller
    public var myName = ""
    /// Something wrong with the app itself (moved, or not signed).
    public var appProblem: String?
    public var configured = false
    public var candidates: [Candidate] = []
    public var helper = Helper()
    public var finished = false
    /// Open at Login was decided once already, so the person's choice stands.
    public var loginDecided = false
    public var asker = Asker()

    public init() {}

    /// The facts in a status reply; nil when it is not one.
    public init?(json: Data) {
        guard let raw = try? JSONDecoder().decode(Raw.self, from: json) else { return nil }
        role = raw.role ?? .controller
        myName = raw.me?.realName ?? raw.me?.name ?? ""
        if raw.app?.installed == false {
            appProblem = raw.app?.error ?? "Atlas.app must be in Applications"
        } else if raw.app?.signed == false {
            appProblem = "This Atlas.app is not signed by its developer; install it again"
        }
        configured = raw.configured ?? false
        candidates = raw.candidates ?? []
        finished = raw.finished ?? false
        loginDecided = raw.menubarLogin ?? false
        guard let side = raw.helper else { return }
        switch role {
        case .controller:
            helper.exists = side.exists ?? false
            helper.name = side.name ?? ""
            helper.realName = side.realName
            helper.administrator = side.administrator
            helper.leftoverSession = side.leftoverSession ?? false
            helper.paired = side.paired ?? false
            helper.signedIn = side.signedIn ?? false
            helper.answering = side.answering ?? false
            helper.onConsole = side.onConsole ?? false
            helper.hidden = side.hidden ?? false
            helper.screenCapture = side.permissions?.screenCapture ?? false
            helper.accessibility = side.permissions?.accessibility ?? false
        case .helper:
            asker.uid = side.controller?.uid ?? 0
            asker.name = side.controller?.name ?? ""
            asker.realName = side.controller?.realName
            asker.installed = side.installed ?? false
            asker.running = side.running ?? false
            asker.screenCapture = side.permissions?.screenCapture ?? false
            asker.accessibility = side.permissions?.accessibility ?? false
        }
    }

    /// The reply exactly as `src/lib/onboarding.rs` writes it.
    private struct Raw: Decodable {
        struct Me: Decodable {
            let name: String?
            let realName: String?
        }
        struct App: Decodable {
            let installed: Bool?
            let signed: Bool?
            let error: String?
        }
        struct Permissions: Decodable {
            let screenCapture: Bool?
            let accessibility: Bool?
        }
        struct Controller: Decodable {
            let uid: Int?
            let name: String?
            let realName: String?
        }
        struct Side: Decodable {
            let exists: Bool?
            let name: String?
            let realName: String?
            let administrator: Bool?
            let leftoverSession: Bool?
            let paired: Bool?
            let signedIn: Bool?
            let answering: Bool?
            let onConsole: Bool?
            let hidden: Bool?
            let permissions: Permissions?
            let controller: Controller?
            let installed: Bool?
            let running: Bool?
        }
        let role: Role?
        let me: Me?
        let app: App?
        let configured: Bool?
        let candidates: [Candidate]?
        let finished: Bool?
        let menubarLogin: Bool?
        let helper: Side?
    }
}

/// The onboarding's screens, in the order the person meets them.
public enum OnboardingStep: Equatable, Hashable, Sendable {
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
        case .welcome: (0, 6)
        case .chooseHelper, .createAccount: (1, 6)
        case .switchToHelper: (2, 6)
        case .finishing: (3, 6)
        case .agents: (4, 6)
        case .done: (5, 6)
        case .allow: (0, 3)
        case .permissions: (1, 3)
        case .helperDone: (2, 3)
        case .blocked: nil
        }
    }
}

/// What the person has chosen in the window, which the facts cannot say.
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
/// alone, so a window opened half-way, or reopened, lands right.
public func onboardingStep(_ facts: OnboardingFacts, _ choices: OnboardingChoices) -> OnboardingStep {
    if let problem = facts.appProblem { return .blocked(problem) }
    if facts.role == .helper {
        let asker = facts.asker
        if !asker.installed || !asker.running {
            // Asked by an account: allow it (again, over a job that does not
            // run, such as an old Atlas's). Asked by no one: nothing to allow here.
            return asker.name.isEmpty
                ? .blocked(
                    "Atlas isn't running in this account. In your own account, open Atlas and choose this account as the helper again."
                )
                : .allow
        }
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
            "\(helper.title) administers this Mac. Make it a standard account in System Settings › Users & Groups, or choose another account."
        )
    }
    if helper.leftoverSession {
        return .blocked(
            "macOS still runs a session of a deleted account with \(helper.title)'s user ID. Restart the Mac, then open Atlas again."
        )
    }
    if facts.finished { return choices.agentsDone ? .done : .agents }
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
