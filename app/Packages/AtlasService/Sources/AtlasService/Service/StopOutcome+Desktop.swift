extension StopOutcome {
  /// What one desktop's Stop came to, as a Stop of every desktop names it.
  struct Desktop: Decodable, Sendable {
    /// The desktop's name.
    var name = ""
    /// What its Stop came to; one that failed holds nothing and warns of its error.
    var reply = StopOutcome()
    /// Whether its Stop is not certain.
    var uncertain = false

    /// The fields of one desktop's part.
    private enum CodingKeys: String, CodingKey {
      case name = "desktop"
      case reply, uncertain
    }

    /// The fields of a failed desktop's reply.
    private enum FailureKeys: String, CodingKey {
      case ok, error
    }

    /// Decodes one desktop's part. A reply that is a failure reads as a Stop not held, warning of its error.
    ///
    /// - Parameter decoder: The decoder reading one desktop's part.
    /// - Throws: `DecodingError` only when it is not an object.
    init(from decoder: any Decoder) throws {
      let container = try decoder.container(keyedBy: CodingKeys.self)
      name = container.lenientText(.name) ?? ""
      uncertain = container.lenientFlag(.uncertain)
      if let reply = try? container.decode(StopOutcome.self, forKey: .reply) {
        self.reply = reply
      }
      guard let failure = try? container.nestedContainer(keyedBy: FailureKeys.self, forKey: .reply),
        failure.lenientBool(.ok) != true
      else { return }
      let error = failure.lenientText(.error)
      reply.held = false
      reply.warnings = error.map { [$0] } ?? []
    }
  }
}
