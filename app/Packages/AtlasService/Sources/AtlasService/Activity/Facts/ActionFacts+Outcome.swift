extension ActionFacts {
  /// How an action went, as its picture's marks show it.
  public enum Outcome: String, Equatable, Sendable {
    /// It worked.
    case worked
    /// It failed.
    case failed
    /// Whether it finished is not known.
    case uncertain
    /// Nothing was sent.
    case notSent
  }
}
