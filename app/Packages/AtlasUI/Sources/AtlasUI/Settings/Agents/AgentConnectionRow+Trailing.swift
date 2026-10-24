extension AgentConnectionRow {
  /// What stands at an agent row's end.
  enum Trailing: Equatable, Sendable {
    /// A green check and Connected.
    case connected
    /// The one Connect button: Connect, or Connecting… with a spinner while the connection runs.
    case button(isWorking: Bool, isEnabled: Bool)
    /// Why the agent can't be connected, such as "Not installed".
    case note(String)
  }
}
