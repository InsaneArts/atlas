extension AgentConnectionRow {
  /// What stands at an agent row's end.
  enum Trailing: Equatable, Sendable {
    /// The connection's switch: on while any part uses Atlas, so turning it off disconnects them all, and off while
    /// none does, so turning it on connects; waiting while it cannot change.
    case toggle(isOn: Bool, isEnabled: Bool)
    /// A spinner beside the switch, on and waiting, while the connection runs.
    case working
    /// Nothing: the agent isn't on this Mac.
    case none
    /// Where the agent stands while it isn't known yet, such as "Checking…", in place of a switch that would say off.
    case note(String)
  }
}
