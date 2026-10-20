/// One agent the person can connect to Atlas.
package struct OnboardingAgentItem: Equatable, Sendable, Identifiable {
  /// The agent's id, such as `claude`: what its switch sends.
  package var id: String
  /// The agent's name.
  package var title: String
  /// Whether it is connected, found or not installed, in words.
  package var detail: String
  /// Whether its switch is on.
  package var isOn: Bool
  /// Whether its switch can be changed: only for an agent found and not yet connected.
  package var isEnabled: Bool

  /// Creates an agent's row.
  ///
  /// - Parameters:
  ///   - id: The agent's id, such as `claude`: what its switch sends.
  ///   - title: The agent's name.
  ///   - detail: Whether it is connected, found or not installed, in words.
  ///   - isOn: Whether its switch is on.
  ///   - isEnabled: Whether its switch can be changed: only for an agent found and not yet connected.
  package init(id: String, title: String, detail: String, isOn: Bool, isEnabled: Bool) {
    self.id = id
    self.title = title
    self.detail = detail
    self.isOn = isOn
    self.isEnabled = isEnabled
  }
}
