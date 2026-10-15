import Foundation

/// How a mode reads: a label, the tone of its dot, the mark the menu bar
/// glyph carries, and a one-line reason. Colour is chosen by the UI.
public struct Mode: Equatable, Sendable {
    public enum Tone: Sendable { case ready, active, attention, quiet }
    public enum Mark: Sendable { case none, working, filled, alert, dash, dimmed }

    public let label: String
    public let tone: Tone
    public let mark: Mark

    public init(label: String, tone: Tone, mark: Mark) {
        self.label = label
        self.tone = tone
        self.mark = mark
    }

    /// The helper's mode, as the person reads it.
    public static func of(_ mode: HelperMode) -> Mode {
        switch mode {
        case .observing: Mode(label: "Watching", tone: .ready, mark: .none)
        case .agent: Mode(label: "Agent working", tone: .active, mark: .working)
        case .controlling: Mode(label: "Viewer has control", tone: .active, mark: .filled)
        case .recovering: Mode(label: "Needs you", tone: .attention, mark: .alert)
        case .blocked: Mode(label: "Not ready", tone: .attention, mark: .dash)
        case .unavailable: Mode(label: "Unavailable", tone: .quiet, mark: .dimmed)
        }
    }

    /// The mode as a viewer sees it: the service knows only that some viewer
    /// holds control; a viewer knows whether it is that one.
    public static func seen(by overview: ViewerStatus.Overview, holdingControl: Bool) -> Mode {
        let mode = overview.mode
        if holdingControl && mode != .recovering && mode != .unavailable {
            return Mode(label: "You have control", tone: .active, mark: .filled)
        }
        if mode == .controlling && !holdingControl {
            return Mode(label: "Another viewer has control", tone: .active, mark: .filled)
        }
        return of(mode)
    }
}

/// Words for times, operations and facts, shared by the panel and viewer.
public enum Words {
    /// Seconds ago as a short relative time.
    public static func ago(_ seconds: Double) -> String {
        if seconds < 5 { return "just now" }
        if seconds < 90 { return "\(Int(seconds.rounded())) s ago" }
        if seconds < 5400 { return "\(Int((seconds / 60).rounded())) min ago" }
        return "\(Int((seconds / 3600).rounded())) h ago"
    }

    /// What an operation did. No payload is ever recorded.
    public static func describe(_ row: ViewerStatus.Activity) -> String {
        if !row.ok { return "\(row.operation) failed" }
        let count = row.actionCount ?? 0
        switch row.operation {
        case "input" where count > 0: return "sent \(count) action\(count == 1 ? "" : "s")"
        case "run": return "ran a command"
        case "restart": return "recovered"
        default: return row.operation
        }
    }

    /// "1512 × 982", or nil when unknown.
    public static func size(_ size: ViewerStatus.Size?) -> String? {
        guard let size, size.width > 0, size.height > 0 else { return nil }
        return "\(Int(size.width.rounded())) × \(Int(size.height.rounded()))"
    }

    /// How fresh the picture is, from the age of the last one.
    public static func freshness(_ age: Double?) -> String {
        guard let age, age.isFinite else { return "Waiting for the first picture" }
        if age < Freshness.live { return "Live" }
        if age < Freshness.stale { return "Updated \(Int(age.rounded())) s ago" }
        return "Picture is stale"
    }
}

/// How old a picture may be, in seconds: the header and control agree on it.
public enum Freshness {
    /// Younger than this reads as live.
    public static let live: Double = 3
    /// This old, the picture is stale: the header says so and control ends.
    public static let stale: Double = 6.5
}

/// One row of recent activity, as shown.
public struct ActivityItem: Equatable, Sendable, Identifiable {
    public let id: Int
    /// "You" or "Agent".
    public let who: String
    public let text: String
    public let when: String
    public let ok: Bool
    /// Seconds ago, when known.
    public let age: Double?

    public var line: String { "\(who) \(text)" }
}

/// One check in the viewer sidebar's Ready card.
public struct StatusRow: Equatable, Sendable, Identifiable {
    public enum State: Sendable { case ready, unknown, bad }
    public let name: String
    public let state: State
    public let text: String
    public var id: String { name }
}

/// The recovery alert: what it says, and whether recovery is running.
public struct Recovery: Equatable, Sendable {
    public let title: String
    public let reason: String
    public let busy: Bool
}

/// What the panel and viewer show of a status: one place, so they agree.
public struct Overview: Equatable, Sendable {
    public let status: ViewerStatus

    public init(_ status: ViewerStatus) {
        self.status = status
    }

    /// Recent operations, newest first.
    public func activity(limit: Int = 6) -> [ActivityItem] {
        status.overview.activity.prefix(limit).enumerated().map { index, row in
            ActivityItem(
                id: index, who: row.actor == "viewer" ? "You" : "Agent", text: Words.describe(row),
                when: row.ageSeconds.map(Words.ago) ?? "", ok: row.ok, age: row.ageSeconds)
        }
    }

    /// The few latest actions shown over the desktop, oldest first, while recent.
    public func captions(limit: Int = 3, within seconds: Double = 60) -> [ActivityItem] {
        Array(activity(limit: 12).filter { ($0.age ?? .infinity) <= seconds }.prefix(limit).reversed())
    }

    /// The newest action in one line, for the panel.
    public var lastAction: String {
        guard let first = activity(limit: 1).first else { return "No recent actions" }
        return [first.line, first.when].filter { !$0.isEmpty }.joined(separator: " · ")
    }

    /// Label and value rows about the session; this viewer knows whether
    /// it holds control.
    public func facts(holdingControl: Bool) -> [(String, String)] {
        let overview = status.overview
        let input =
            status.session.inputReady ? "Ready" : status.session.inputReachable ? "Not ready" : "Unavailable"
        return [
            ("Display", Words.size(overview.display) ?? "Unknown"),
            ("Input", input),
            ("Control", availability(holdingControl: holdingControl)),
            ("Pending actions", overview.pending > 0 ? String(overview.pending) : "None"),
        ]
    }

    /// Whether this viewer could take control now, and if not, why.
    func availability(holdingControl: Bool) -> String {
        if holdingControl { return "This window" }
        return switch status.overview.mode {
        case .observing: "Available"
        case .controlling: "Another window"
        case .agent: "Agent working"
        case .recovering: "After recovery"
        case .blocked: "Not ready"
        case .unavailable: "Unavailable"
        }
    }

    /// Why taking control must wait, if it must.
    public func controlBlocked(holdingControl: Bool) -> String? {
        switch status.overview.mode {
        case .agent: "An agent is acting in the helper"
        case .recovering: "Recover before taking control"
        case .controlling where !holdingControl: "Another viewer has control"
        default: nil
        }
    }

    /// The sidebar's sentence: why the helper is in its mode.
    public func reason(holdingControl: Bool) -> String {
        let mode = status.overview.mode
        if holdingControl && mode != .recovering && mode != .unavailable {
            return "You have mouse and keyboard control"
        }
        if mode == .controlling { return "Another viewer has control" }
        return status.overview.reason.isEmpty ? "Connecting to the helper" : status.overview.reason
    }

    /// Each readiness check, then the native Save and Open check, which
    /// never counts as ready. An unknown check is unobservable, not failed.
    public var statusRows: [StatusRow] {
        var rows = status.checks.map { check in
            switch check.ok {
            case true: StatusRow(name: check.name, state: .ready, text: "Ready")
            case nil: StatusRow(name: check.name, state: .unknown, text: "Unknown. \(check.nextStep ?? "")")
            default: StatusRow(name: check.name, state: .bad, text: check.nextStep ?? "Not ready")
            }
        }
        let native = status.capabilities.nativeDialogs
        let age = native.ageSeconds.map(Words.ago) ?? ""
        switch native.state {
        case "passed":
            rows.append(StatusRow(name: "Native dialogs", state: .ready, text: "Save and Open passed \(age)"))
        case "failed":
            let stage = native.failedAt.map { " at “\($0)”" } ?? ""
            rows.append(StatusRow(name: "Native dialogs", state: .bad, text: "Failed \(age)\(stage). \(native.reason)"))
        default:
            if !native.reason.isEmpty {
                rows.append(StatusRow(name: "Native dialogs", state: .unknown, text: native.reason))
            }
        }
        return rows
    }

    /// Recovery is offered only without control, when something needs it.
    func recoveryOffered(holdingControl: Bool) -> Bool {
        !holdingControl && (status.overview.mode == .recovering || status.overview.mode == .blocked)
    }

    /// The recovery alert, if any: while recovering, or when offered.
    public func recovery(holdingControl: Bool, recovering: Bool) -> Recovery? {
        if recovering {
            return Recovery(title: "Recovering the helper", reason: "Restoring the helper.", busy: true)
        }
        guard recoveryOffered(holdingControl: holdingControl) else { return nil }
        let title =
            status.overview.mode == .blocked ? "The helper isn't ready" : "The last action's outcome is unknown"
        return Recovery(title: title, reason: reason(holdingControl: false), busy: false)
    }
}
