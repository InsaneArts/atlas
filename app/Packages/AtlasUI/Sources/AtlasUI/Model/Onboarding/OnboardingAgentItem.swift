/// One agent the person can connect to Atlas.
public struct OnboardingAgentItem: Equatable, Sendable, Identifiable {
  /// The agent's id, such as `claude`: what its switch sends.
  public var id: String
  /// The agent's name.
  public var title: String
  /// Where it stands, in a few words: found, getting the Atlas skill, connected or not on this Mac.
  public var detail: String
  /// Its icon: its app's own where the app is on this Mac, else its tile, as Settings shows it.
  public var icon: RowIcon
  /// Whether its switch is on.
  public var isOn: Bool
  /// Whether its switch can be changed: only for an agent found and not yet connected.
  public var isEnabled: Bool

  /// Creates an agent's row.
  ///
  /// - Parameters:
  ///   - id: The agent's id, such as `claude`: what its switch sends.
  ///   - title: The agent's name.
  ///   - detail: Where it stands, in a few words.
  ///   - icon: Its icon, as Settings shows it.
  ///   - isOn: Whether its switch is on.
  ///   - isEnabled: Whether its switch can be changed: only for an agent found and not yet connected.
  public init(id: String, title: String, detail: String, icon: RowIcon, isOn: Bool, isEnabled: Bool) {
    self.id = id
    self.title = title
    self.detail = detail
    self.icon = icon
    self.isOn = isOn
    self.isEnabled = isEnabled
  }
}
