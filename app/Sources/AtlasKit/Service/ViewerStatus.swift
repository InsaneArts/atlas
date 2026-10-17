import Foundation

/// What the helper is doing, as the service names it. A name this app does
/// not know reads as unavailable.
public enum HelperMode: String, Decodable, Sendable {
    case observing, agent, controlling, recovering, blocked, unavailable
}

/// The viewer service's `status.json`: the helper's readiness, its session
/// and the overview the panel and viewer show. Every field may be missing
/// (the service could not reach the helper), so each has a safe default.
public struct ViewerStatus: Decodable, Equatable, Sendable {
    public var ready = false
    public var canObserve = false
    public var checks: [Check] = []
    public var nextSteps: [String] = []
    public var error: String?
    public var session = Session()
    public var capabilities = Capabilities()
    public var overview = Overview()

    public init() {}

    public struct Check: Decodable, Equatable, Sendable {
        public var name: String
        /// Unknown (`nil`) when it cannot be observed yet; never reported as failed.
        public var ok: Bool?
        public var nextStep: String?

        public init(name: String, ok: Bool?, nextStep: String? = nil) {
            self.name = name
            self.ok = ok
            self.nextStep = nextStep
        }
    }

    public struct Session: Decodable, Equatable, Sendable {
        public var userName: String?
        public var inputReady = false
        public var inputReachable = false

        public init() {}

        enum CodingKeys: String, CodingKey { case user, inputService }
        struct User: Decodable { var name: String? }

        struct Input: Decodable {
            var ready: Bool?
            var ok: Bool?
        }

        public init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            userName = (try? container.decodeIfPresent(User.self, forKey: .user))?.name
            let input = try? container.decodeIfPresent(Input.self, forKey: .inputService)
            inputReady = input?.ready == true
            inputReachable = input?.ok == true
        }
    }

    public struct Capabilities: Decodable, Equatable, Sendable {
        public var nativeDialogs = NativeDialogs()

        public init() {}

        public struct NativeDialogs: Decodable, Equatable, Sendable {
            public var state = "unknown"
            public var reason = ""
            public var ageSeconds: Double?
            public var failedAt: String?

            public init() {}

            enum CodingKeys: String, CodingKey { case state, reason, ageSeconds, checks }

            struct Stage: Decodable {
                var stage: String?
                var passed: Bool?
            }

            public init(from decoder: any Decoder) throws {
                let container = try decoder.container(keyedBy: CodingKeys.self)
                state = (try? container.decodeIfPresent(String.self, forKey: .state)) ?? "unknown"
                reason = (try? container.decodeIfPresent(String.self, forKey: .reason)) ?? ""
                ageSeconds = try? container.decodeIfPresent(Double.self, forKey: .ageSeconds)
                let stages = (try? container.decodeIfPresent([Stage].self, forKey: .checks)) ?? []
                failedAt = stages.first { $0.passed != true }?.stage
            }
        }

        enum CodingKeys: String, CodingKey { case nativeDialogs }

        public init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            nativeDialogs =
                (try? container.decodeIfPresent(NativeDialogs.self, forKey: .nativeDialogs)) ?? NativeDialogs()
        }
    }

    /// What is happening in the helper, as the service sums it up.
    public struct Overview: Decodable, Equatable, Sendable {
        public var mode = HelperMode.unavailable
        public var reason = "Connecting to the helper"
        public var account: String?
        /// The account's full name ("Atlas"), when it has one.
        public var accountName: String?
        public var display: Size?
        /// Actions of unknown outcome, waiting for recovery.
        public var pending = 0
        public var activity: [Activity] = []

        public init() {}

        enum CodingKeys: String, CodingKey {
            case mode, reason, account, accountName, display, pending, activity
        }

        public init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            mode = (try? container.decodeIfPresent(HelperMode.self, forKey: .mode)) ?? .unavailable
            reason = (try? container.decodeIfPresent(String.self, forKey: .reason)) ?? reason
            account = try? container.decodeIfPresent(String.self, forKey: .account)
            accountName = try? container.decodeIfPresent(String.self, forKey: .accountName)
            display = try? container.decodeIfPresent(Size.self, forKey: .display)
            pending = (try? container.decodeIfPresent(Int.self, forKey: .pending)) ?? 0
            activity = (try? container.decodeIfPresent([Activity].self, forKey: .activity)) ?? []
        }
    }

    public struct Size: Decodable, Equatable, Sendable {
        public var width: Double
        public var height: Double

        public init(width: Double, height: Double) {
            self.width = width
            self.height = height
        }
    }

    /// One recorded operation. No payload is ever recorded.
    public struct Activity: Decodable, Equatable, Sendable {
        public var operation: String
        public var ok: Bool
        public var actionCount: Int?
        /// "viewer" when the person did it through a viewer, else an agent.
        public var actor: String
        public var ageSeconds: Double?

        public init(
            operation: String, ok: Bool, actionCount: Int? = nil, actor: String = "agent", ageSeconds: Double? = nil
        ) {
            self.operation = operation
            self.ok = ok
            self.actionCount = actionCount
            self.actor = actor
            self.ageSeconds = ageSeconds
        }

        enum CodingKeys: String, CodingKey { case operation, ok, actionCount, actor, ageSeconds }

        public init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            operation = (try? container.decodeIfPresent(String.self, forKey: .operation)) ?? "action"
            ok = (try? container.decodeIfPresent(Bool.self, forKey: .ok)) == true
            actionCount = try? container.decodeIfPresent(Int.self, forKey: .actionCount)
            actor = (try? container.decodeIfPresent(String.self, forKey: .actor)) ?? "agent"
            ageSeconds = try? container.decodeIfPresent(Double.self, forKey: .ageSeconds)
        }
    }

    enum CodingKeys: String, CodingKey {
        case ready, canObserve, checks, nextSteps, error, session, capabilities, overview
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        ready = (try? container.decodeIfPresent(Bool.self, forKey: .ready)) == true
        canObserve = (try? container.decodeIfPresent(Bool.self, forKey: .canObserve)) == true
        checks = (try? container.decodeIfPresent([Check].self, forKey: .checks)) ?? []
        nextSteps = (try? container.decodeIfPresent([String].self, forKey: .nextSteps)) ?? []
        error = try? container.decodeIfPresent(String.self, forKey: .error)
        session = (try? container.decodeIfPresent(Session.self, forKey: .session)) ?? Session()
        capabilities = (try? container.decodeIfPresent(Capabilities.self, forKey: .capabilities)) ?? Capabilities()
        overview = (try? container.decodeIfPresent(Overview.self, forKey: .overview)) ?? Overview()
    }
}
