/// The main window's places, in the order the View menu numbers them: the Screen, then Activity.
public enum ViewerPlace: Hashable, Sendable, CaseIterable {
  /// The helper's screen.
  case screen
  /// What was done in the helper.
  case activity
}
