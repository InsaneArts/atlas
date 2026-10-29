extension ViewerStatus {
  /// One recorded operation. No typed text, command or coordinates are ever recorded.
  public struct Activity: Decodable, Equatable, Sendable {
    /// The row's id, which the service never gives another row and which holds across readings, from builds that
    /// send it.
    public var id: String?
    /// The operation's name, such as `input` or `run`.
    public var operation: String
    /// Whether it worked.
    public var ok: Bool
    /// How many input actions it carried, for `input`.
    public var actionCount: Int?
    /// "viewer" when the person did it through a viewer, else an agent.
    public var actor: String
    /// How long ago it ran, in seconds.
    public var ageSeconds: Double?
    /// What it did in the service's words, such as "clicked Save in TextEdit", from builds that say so.
    public var says: String?
    /// The key of the host session it came from; nil for the person's own and for commands no host named, never empty.
    public var session: String?
    /// Whether it was refused before anything was sent, as when another app was in front: nothing to recover, and
    /// not a problem.
    public var nothingSent: Bool
    /// Whether its outcome is unknown until the person recovers.
    public var uncertain: Bool

    /// The key of the host session an agent's operation came from: Terminal's empty key for one no host named.
    public var sessionKey: String { session ?? "" }

    /// Creates an operation.
    ///
    /// - Parameters:
    ///   - id: The row's id.
    ///   - operation: The operation's name.
    ///   - ok: Whether it worked.
    ///   - actionCount: How many input actions it carried.
    ///   - actor: "viewer" for the person, else an agent.
    ///   - ageSeconds: How long ago it ran, in seconds.
    ///   - says: What it did in the service's words.
    ///   - session: The key of the host session it came from.
    ///   - nothingSent: Whether it was refused before anything was sent.
    ///   - uncertain: Whether its outcome is unknown.
    public init(
      id: String? = nil,
      operation: String,
      ok: Bool,
      actionCount: Int? = nil,
      actor: String = "agent",
      ageSeconds: Double? = nil,
      says: String? = nil,
      session: String? = nil,
      nothingSent: Bool = false,
      uncertain: Bool = false
    ) {
      self.id = id
      self.operation = operation
      self.ok = ok
      self.actionCount = actionCount
      self.actor = actor
      self.ageSeconds = ageSeconds
      self.says = says
      self.session = session
      self.nothingSent = nothingSent
      self.uncertain = uncertain
    }

    /// The fields of one `activity` row.
    enum CodingKeys: String, CodingKey {
      /// Each field by the name the service writes.
      case id, operation, ok, actionCount, actor, ageSeconds, says, session, nothingSent, uncertain
    }

    /// Decodes an operation; one that names nothing reads as an agent's failed "action".
    ///
    /// - Parameter decoder: The decoder reading one `activity` row.
    /// - Throws: `DecodingError` only when the row is not an object.
    public init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      // Each field is read leniently: a missing or mistyped one takes its default, so one odd field never drops the row.
      id = (try? container.decodeIfPresent(String.self, forKey: .id))
        .flatMap { $0?.isEmpty == false ? $0 : nil }
      operation = (try? container.decodeIfPresent(String.self, forKey: .operation)) ?? "action"
      ok = (try? container.decodeIfPresent(Bool.self, forKey: .ok)) == true
      actionCount = try? container.decodeIfPresent(Int.self, forKey: .actionCount)
      actor = (try? container.decodeIfPresent(String.self, forKey: .actor)) ?? "agent"
      ageSeconds = (try? container.decodeIfPresent(Double.self, forKey: .ageSeconds))
        .flatMap { $0.isFinite ? $0 : nil }
      let said = (try? container.decodeIfPresent(String.self, forKey: .says)) ?? nil
      says = said?.isEmpty == false ? said : nil
      // An empty key is Terminal's, as a missing one is, so both read as none.
      session = (try? container.decodeIfPresent(String.self, forKey: .session))
        .flatMap { $0?.isEmpty == false ? $0 : nil }
      nothingSent = (try? container.decodeIfPresent(Bool.self, forKey: .nothingSent)) == true
      uncertain = (try? container.decodeIfPresent(Bool.self, forKey: .uncertain)) == true
    }
  }
}
