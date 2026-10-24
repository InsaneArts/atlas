extension ViewerStatus {
  /// One recorded operation. No typed text, command or coordinates are ever recorded.
  package struct Activity: Decodable, Equatable, Sendable {
    /// The row's id, which the service never gives another row and which holds across readings, from builds that
    /// send it.
    package var id: String?
    /// The operation's name, such as `input` or `run`.
    package var operation: String
    /// Whether it worked.
    package var ok: Bool
    /// How many input actions it carried, for `input`.
    package var actionCount: Int?
    /// "viewer" when the person did it through a viewer, else an agent.
    package var actor: String
    /// How long ago it ran, in seconds.
    package var ageSeconds: Double?
    /// What it did in the service's words, such as "clicked Save in TextEdit", from builds that say so.
    package var says: String?

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
    package init(
      id: String? = nil,
      operation: String,
      ok: Bool,
      actionCount: Int? = nil,
      actor: String = "agent",
      ageSeconds: Double? = nil,
      says: String? = nil
    ) {
      self.id = id
      self.operation = operation
      self.ok = ok
      self.actionCount = actionCount
      self.actor = actor
      self.ageSeconds = ageSeconds
      self.says = says
    }

    /// The fields of one `activity` row.
    enum CodingKeys: String, CodingKey { case id, operation, ok, actionCount, actor, ageSeconds, says }

    /// Decodes an operation; one that names nothing reads as an agent's failed "action".
    ///
    /// - Parameter decoder: The decoder reading one `activity` row.
    /// - Throws: `DecodingError` only when the row is not an object.
    package init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      id = (try? container.decodeIfPresent(String.self, forKey: .id))
        .flatMap { $0?.isEmpty == false ? $0 : nil }
      operation = (try? container.decodeIfPresent(String.self, forKey: .operation)) ?? "action"
      ok = (try? container.decodeIfPresent(Bool.self, forKey: .ok)) == true
      actionCount = try? container.decodeIfPresent(Int.self, forKey: .actionCount)
      actor = (try? container.decodeIfPresent(String.self, forKey: .actor)) ?? "agent"
      ageSeconds = try? container.decodeIfPresent(Double.self, forKey: .ageSeconds)
      let said = (try? container.decodeIfPresent(String.self, forKey: .says)) ?? nil
      says = said?.isEmpty == false ? said : nil
    }
  }
}
