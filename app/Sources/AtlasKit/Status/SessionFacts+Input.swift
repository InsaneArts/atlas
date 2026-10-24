extension SessionFacts {
  /// Whether the helper's input is ready.
  package enum Input: Equatable, Sendable {
    /// The input service is ready.
    case ready
    /// The input service answers but is not ready.
    case notReady
    /// The input service does not answer.
    case notAnswering
  }
}
