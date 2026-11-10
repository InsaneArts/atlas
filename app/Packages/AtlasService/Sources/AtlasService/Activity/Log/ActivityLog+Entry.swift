import Foundation

extension ActivityLog {
  /// One thing done in the helper, as shown: no command or window title, and typed text and coordinates only in the
  /// facts that mark its pictures.
  public struct Entry: Decodable, Equatable, Sendable, Identifiable {
    /// Names the row; never reused for another. A running command's row is replaced by one with a new id.
    public var id: String
    /// Names the action across command steps or a growing personal input run; the row id for older services.
    public var logicalId: String
    /// What was done, in the service's words, such as "typed in TextEdit"; nil when it gives none.
    public var says: String?
    /// How it went.
    public var outcome: Outcome
    /// Who did it.
    public var actor: Actor
    /// How long ago it was done, in seconds, when known.
    var ageSeconds: Double?
    /// When it started, to the millisecond, when the service says: the same at every reading, where an age counted
    /// back from a reading moves by up to a second.
    var startedAt: Date?
    /// Why it failed or what held it, as a short plain phrase, when the service says.
    public var reason: String?
    /// The operation's name, such as `input` or `run`.
    public var operation: String
    /// How many input actions it carried, for `input`.
    var count: Int?
    /// The key of the host session it came from; nil for the person's own and for commands no host named, never empty.
    public var session: String?
    /// The capture a screenshot row took, by Atlas's own file name, when the service says.
    public var capture: String?
    /// The verb Atlas wrote for the row, such as `click`, `type` or `key`, when the service says.
    public var verb: String?
    /// The name of the app it acted in, when the service says.
    public var app: String?
    /// The control it named, such as a button's title, or the keys it pressed, such as `⌘S`, when the service says.
    public var label: String?
    /// The bundle identifier of the app it acted in, such as `com.apple.TextEdit`, when the service says.
    public var bundleId: String?
    /// A key press's chords as keycaps, such as `[["⌘", "S"]]`; none for any other row.
    public var keys: [[String]]
    /// The picture taken right before an agent's action, by Atlas's own file name, when the service says.
    public var before: String?
    /// The picture of the screen right after it, by its file name: one Atlas took after it, or the next action's
    /// picture before, when the service says.
    public var after: String?
    /// When the picture after it was taken; nil when there is none or the service did not say.
    public var afterAt: Date?
    /// What Atlas recorded to mark the spot on its pictures; nil when it recorded nothing.
    public var facts: ActionFacts?
    /// The listed account an agent's action acted in, when the service says; none for older rows and for actions in
    /// no listed account.
    public var account: ActivityLog.Account?
    /// What let it act there: allowed once or for the task, or a Trusted account; only with an account.
    public var allowed: ActivityLog.Allowance?

    /// The key of the host session an agent's row came from: Terminal's empty key for one no host named.
    public var sessionKey: String { session ?? "" }

    /// The group the row is listed under while every session's work shows: its session's key, Terminal's empty one
    /// for an agent's row no host named, or nil for the person's own.
    public var groupKey: String? { actor == .you ? nil : sessionKey }

    /// Creates a row.
    ///
    /// - Parameters:
    ///   - id: Names the row.
    ///   - logicalId: Names the action; the row id when none is supplied.
    ///   - says: What was done, in the service's words.
    ///   - outcome: How it went.
    ///   - actor: Who did it.
    ///   - ageSeconds: How long ago, in seconds.
    ///   - startedAt: When it started.
    ///   - reason: Why it failed or what held it.
    ///   - operation: The operation's name.
    ///   - count: How many input actions it carried.
    ///   - session: The key of the host session it came from.
    ///   - capture: The capture a screenshot row took, by its file name.
    ///   - verb: The verb Atlas wrote for the row.
    ///   - app: The name of the app it acted in.
    ///   - label: The control it named, or the keys it pressed.
    ///   - bundleId: The bundle identifier of the app it acted in.
    ///   - keys: A key press's chords as keycaps.
    ///   - before: The picture taken right before an agent's action, by its file name.
    ///   - after: The picture of the screen right after it, by its file name.
    ///   - afterAt: When the picture after it was taken.
    ///   - facts: What Atlas recorded to mark the spot on its pictures.
    ///   - account: The listed account it acted in.
    ///   - allowed: What let it act there; only with an account.
    public init(
      id: String,
      logicalId: String? = nil,
      says: String? = nil,
      outcome: Outcome = .worked,
      actor: Actor = .agent,
      ageSeconds: Double? = nil,
      startedAt: Date? = nil,
      reason: String? = nil,
      operation: String = "input",
      count: Int? = nil,
      session: String? = nil,
      capture: String? = nil,
      verb: String? = nil,
      app: String? = nil,
      label: String? = nil,
      bundleId: String? = nil,
      keys: [[String]] = [],
      before: String? = nil,
      after: String? = nil,
      afterAt: Date? = nil,
      facts: ActionFacts? = nil,
      account: ActivityLog.Account? = nil,
      allowed: ActivityLog.Allowance? = nil
    ) {
      self.id = id
      self.logicalId = logicalId ?? id
      self.says = says
      self.outcome = outcome
      self.actor = actor
      self.ageSeconds = ageSeconds
      self.startedAt = startedAt
      self.reason = reason
      self.operation = operation
      self.count = count
      self.session = session
      self.capture = capture
      self.verb = verb
      self.app = app
      self.label = label
      self.bundleId = bundleId
      self.keys = keys
      self.before = before
      self.after = after
      self.afterAt = afterAt
      self.facts = facts
      self.account = account
      self.allowed = account == nil ? nil : allowed
    }

    /// The fields of one `activity` row this app reads.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case id, says, ok, started, actor, ageSeconds, startedAt, reason, operation, count, session, capture, nothingSent
      /// The fields that name what it did and where.
      case verb, app, label, bundleId, keys
      /// The pictures taken right before and right after it, and what was recorded to mark them.
      case before, after, afterAt, facts
      /// Whether input was partly sent before a refusal.
      case partial
      /// The action's identity across successive rows.
      case logicalId
      /// The listed account it acted in, and what let it act there.
      case account, allowed
    }

    /// Decodes a row, keeping a safe default for anything missing or of the wrong type: a row that says nothing
    /// reads as an agent's failed "action", and empty words read as none.
    ///
    /// - Parameter decoder: The decoder reading one `activity` row.
    /// - Throws: `DecodingError` only when the row is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      id = container.lenientText(.id) ?? ""
      logicalId = container.lenientText(.logicalId) ?? id
      says = container.lenientText(.says)
      outcome = Outcome(
        started: container.lenientFlag(.started),
        ok: container.lenientFlag(.ok),
        nothingSent: container.lenientFlag(.nothingSent),
        partial: container.lenientFlag(.partial)
      )
      actor = Actor(word: container.lenientText(.actor))
      ageSeconds = try? container.decodeIfPresent(Double.self, forKey: .ageSeconds)
      startedAt = container.lenientDate(milliseconds: .startedAt)
      reason = container.lenientText(.reason)
      operation = container.lenientText(.operation) ?? "action"
      count = try? container.decodeIfPresent(Int.self, forKey: .count)
      // An empty key is Terminal's, as a missing one is, so both read as none.
      session = container.lenientText(.session)
      capture = container.lenientText(.capture)
      verb = container.lenientText(.verb)
      app = container.lenientText(.app)
      label = container.lenientText(.label)
      bundleId = container.lenientText(.bundleId)
      keys = (try? container.decodeIfPresent([[String]].self, forKey: .keys)) ?? []
      before = container.lenientText(.before)
      after = container.lenientText(.after)
      afterAt = after == nil ? nil : container.lenientDate(milliseconds: .afterAt)
      facts = try? container.decodeIfPresent(ActionFacts.self, forKey: .facts)
      // An account that does not read whole is none, and what let it act counts only with one.
      account = container.lenientAccount(.account)
      allowed = account == nil ? nil : container.lenientAllowance(.allowed)
    }
  }
}
