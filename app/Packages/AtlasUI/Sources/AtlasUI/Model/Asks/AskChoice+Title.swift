extension AskChoice {
  /// The answer's words on its button.
  public var title: String {
    switch self {
    case .task: "Allow for This Task"
    case .once: "Just This Once"
    case .deny: "Deny"
    }
  }
}
