/// What the main window's card shows between its header and footer: the helper's screen, or Activity.
public enum ViewerPane: Equatable, Sendable {
  /// The Screen tab, drawn from ``ViewerState/screen``.
  case screen
  /// What was done in the helper.
  ///
  /// - Parameter state: The Activity pane.
  case activity(ViewerActivityState)
}
