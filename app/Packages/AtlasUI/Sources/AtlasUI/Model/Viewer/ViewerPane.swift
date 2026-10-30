/// What the main window's card shows between its header and footer: the helper's screen, or a list's pane.
public enum ViewerPane: Equatable, Sendable {
  /// The Screen tab, drawn from ``ViewerState/screen``.
  case screen
  /// What was done in the helper.
  ///
  /// - Parameter state: The Activity pane.
  case activity(ViewerActivityState)
  /// The apps open in the helper.
  ///
  /// - Parameter state: The Apps pane.
  case apps(ViewerAppsState)
  /// The screenshots agents took.
  ///
  /// - Parameter state: The Screenshots pane.
  case screenshots(ViewerShotsState)
}
