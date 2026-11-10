extension ActivityLog {
  /// What let an agent's action act in a listed account: the person allowed it once or for the task, or the account
  /// is Trusted.
  public enum Allowance: String, Equatable, Sendable, CaseIterable {
    /// The person allowed this one action.
    case once
    /// The person allowed the rest of the agent's task in the account.
    case task
    /// The account is Trusted, so nothing needed allowing.
    case trusted
  }
}
