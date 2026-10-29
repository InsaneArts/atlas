/// The main window's places, in the order the View menu numbers them and the sidebar's Library lists the lists: the
/// Screen, then Activity, Screenshots and Apps.
public enum ViewerPlace: Hashable, Sendable, CaseIterable {
  /// The helper's screen.
  case screen
  /// What was done in the helper.
  case activity
  /// The screenshots agents took.
  case screenshots
  /// The apps open in the helper.
  case apps
}
