import Foundation

/// What the panel shows, decoded from the viewer's status report.
public struct Snapshot: Equatable, Sendable {
    public var mode = "unavailable"
    public var reason = "Connecting to the helper"
    public var account = "Helper account"
    public var display = ""
    public var last = "No recent actions"
    public var when = ""

    public var controlling: Bool { mode == "controlling" }

    public init() {}

    public init(status: [String: Any]) {
        let overview = status["overview"] as? [String: Any] ?? [:]
        mode = overview["mode"] as? String ?? "unavailable"
        reason = overview["reason"] as? String ?? reason
        account = overview["account"] as? String ?? account
        if let display = overview["display"] as? [String: Any],
            let width = display["width"] as? Double, let height = display["height"] as? Double
        {
            self.display = "\(Int(width)) × \(Int(height))"
        }
        if let row = (overview["activity"] as? [[String: Any]])?.first,
            let operation = row["operation"] as? String
        {
            let who = row["actor"] as? String == "viewer" ? "You" : "Agent"
            last = "\(who) \(Snapshot.describe(operation, row))"
            when = (row["ageSeconds"] as? Double).map(Snapshot.ago) ?? ""
        }
    }

    /// What an operation did, in words; no payload is ever recorded.
    public static func describe(_ operation: String, _ row: [String: Any]) -> String {
        let count = row["actionCount"] as? Int ?? 0
        if row["ok"] as? Bool != true { return "\(operation) failed" }
        if operation == "input" && count > 0 {
            return "sent \(count) action\(count == 1 ? "" : "s")"
        }
        if operation == "run" { return "ran a command" }
        if operation == "restart" { return "recovered" }
        return operation
    }

    public static func ago(_ seconds: Double) -> String {
        if seconds < 5 { return "Now" }
        if seconds < 90 { return "\(Int(seconds.rounded())) s ago" }
        if seconds < 5400 { return "\(Int((seconds / 60).rounded())) min ago" }
        return "\(Int((seconds / 3600).rounded())) h ago"
    }
}

/// How each mode reads: a label, the tone of its one small dot, and the
/// mark the menu bar glyph carries. Colour is chosen by the UI.
public struct Mode: Equatable, Sendable {
    public enum Tone: Sendable { case ready, active, attention, quiet }
    public enum Mark: Sendable { case none, working, filled, alert, dash, dimmed }

    public let label: String
    public let tone: Tone
    public let mark: Mark

    public static func of(_ mode: String) -> Mode {
        switch mode {
        case "observing": return Mode(label: "Observing", tone: .ready, mark: .none)
        case "agent": return Mode(label: "Agent working", tone: .active, mark: .working)
        case "controlling": return Mode(label: "Viewer has control", tone: .active, mark: .filled)
        case "recovering": return Mode(label: "Needs recovery", tone: .attention, mark: .alert)
        case "blocked": return Mode(label: "Not ready", tone: .attention, mark: .dash)
        default: return Mode(label: "Unavailable", tone: .quiet, mark: .dimmed)
        }
    }
}
