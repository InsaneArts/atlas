extension AgentConnectionRow {
  /// What stands at an agent row's end.
  enum Trailing: Equatable, Sendable {
    /// A green check and Connected.
    case connected
    /// The one Connect button: Connect, or Connecting… with a spinner while the connection runs.
    case button(isWorking: Bool, isEnabled: Bool)
    /// Nothing: the agent isn't on this Mac.
    case none
    /// Where the agent stands while it isn't known yet, such as "Checking…".
    case note(String)
  }
}
