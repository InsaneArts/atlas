/// The panel's white capsule: the one thing to do next in the helper's state, such as Open Atlas or Recover….
package struct PanelMainButton: Equatable, Sendable {
  /// Its word.
  package var title: String
  /// Whether it can be pressed now.
  package var isEnabled: Bool
  /// What it does, for its tooltip.
  package var help: String
  /// What pressing it sends.
  package var event: PanelEvent

  /// Creates the button.
  ///
  /// - Parameters:
  ///   - title: Its word.
  ///   - isEnabled: Whether it can be pressed now.
  ///   - help: What it does, for its tooltip.
  ///   - event: What pressing it sends.
  package init(title: String, isEnabled: Bool = true, help: String, event: PanelEvent) {
    self.title = title
    self.isEnabled = isEnabled
    self.help = help
    self.event = event
  }
}
