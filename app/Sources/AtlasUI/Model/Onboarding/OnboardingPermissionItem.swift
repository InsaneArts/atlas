/// One of the two permissions Atlas needs in the helper, and whether it is on.
package struct OnboardingPermissionItem: Equatable, Sendable, Identifiable {
  /// The permission's id: what opening its settings sends.
  package var id: String
  /// Its name, as System Settings shows it.
  package var title: String
  /// Whether it is on.
  package var isOn: Bool
  /// Whether it is on, in words.
  package var status: String
  /// The button that opens its switch in System Settings.
  package var allowTitle: String
  /// Whether it is the one to allow next, its button showing where the row takes a press.
  package var canAllow: Bool

  /// Creates one permission as the helper shows it.
  ///
  /// - Parameters:
  ///   - id: The permission's id: what opening its settings sends.
  ///   - title: Its name, as System Settings shows it.
  ///   - isOn: Whether it is on.
  ///   - status: Whether it is on, in words.
  ///   - allowTitle: The button that opens its switch in System Settings.
  ///   - canAllow: Whether it is the one to allow next.
  package init(id: String, title: String, isOn: Bool, status: String, allowTitle: String, canAllow: Bool = false) {
    self.id = id
    self.title = title
    self.isOn = isOn
    self.status = status
    self.allowTitle = allowTitle
    self.canAllow = canAllow
  }
}
