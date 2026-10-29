import Foundation

extension ActivityLog {
  /// One thing done in the helper, as shown: no typed text, command, coordinates or window title.
  public struct Entry: Decodable, Equatable, Sendable, Identifiable {
    /// Names the row; never reused for another. A running command's row is replaced by one with a new id.
    public var id: String
    /// What was done, in the service's words, such as "typed in TextEdit"; nil when it gives none.
    var says: String?
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
    /// Whether a refusal before it was done had already sent part of its text, as typing cut short.
    public var partial: Bool

    /// The key of the host session an agent's row came from: Terminal's empty key for one no host named.
    public var sessionKey: String { session ?? "" }

    /// The group the row is listed under while every session's work shows: its session's key, Terminal's empty one
    /// for an agent's row no host named, or nil for the person's own.
    public var groupKey: String? { actor == .you ? nil : sessionKey }

    /// Creates a row.
    ///
    /// - Parameters:
    ///   - id: Names the row.
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
    ///   - partial: Whether a refusal had already sent part of its text.
    public init(
      id: String,
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
      partial: Bool = false
    ) {
      self.id = id
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
      self.partial = partial
    }

    /// The fields of one `activity` row.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case id, says, ok, started, actor, ageSeconds, startedAt, reason, operation, count, session, capture, nothingSent,
        partial
    }

    /// Decodes a row, keeping a safe default for anything missing or of the wrong type: a row that says nothing
    /// reads as an agent's failed "action", and empty words read as none.
    ///
    /// - Parameter decoder: The decoder reading one `activity` row.
    /// - Throws: `DecodingError` only when the row is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      let text = { (key: CodingKeys) in
        (try? container.decodeIfPresent(String.self, forKey: key))
          .flatMap { $0.isEmpty ? nil : $0 }
      }
      let flag = { (key: CodingKeys) in (try? container.decodeIfPresent(Bool.self, forKey: key)) == true }
      id = text(.id) ?? ""
      says = text(.says)
      outcome = Outcome(started: flag(.started), ok: flag(.ok), nothingSent: flag(.nothingSent))
      actor = Actor(word: text(.actor))
      ageSeconds = try? container.decodeIfPresent(Double.self, forKey: .ageSeconds)
      let milliseconds = try? container.decodeIfPresent(Double.self, forKey: .startedAt)
      startedAt = milliseconds.map { Date(timeIntervalSince1970: $0 / 1000) }
      reason = text(.reason)
      operation = text(.operation) ?? "action"
      count = try? container.decodeIfPresent(Int.self, forKey: .count)
      // An empty key is Terminal's, as a missing one is, so both read as none.
      session = text(.session)
      capture = text(.capture)
      partial = flag(.partial)
    }
  }
}
