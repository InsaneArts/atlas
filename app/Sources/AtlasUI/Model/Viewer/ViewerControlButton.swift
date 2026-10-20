/// The footer's white capsule: Take Control, Connecting… while asked, or Give Back while in control.
package struct ViewerControlButton: Equatable, Sendable {
  /// The button's title.
  package var title: String
  /// Whether it can be pressed.
  package var isEnabled: Bool
  /// Its tooltip, with its shortcut.
  package var help: String
  /// What pressing it, or Control-Shift-Escape, sends.
  package var event: ViewerEvent

  /// Creates the control capsule.
  ///
  /// - Parameters:
  ///   - title: The button's title.
  ///   - isEnabled: Whether it can be pressed.
  ///   - help: Its tooltip, with its shortcut.
  ///   - event: What pressing it, or Control-Shift-Escape, sends.
  package init(title: String, isEnabled: Bool, help: String, event: ViewerEvent) {
    self.title = title
    self.isEnabled = isEnabled
    self.help = help
    self.event = event
  }
}
