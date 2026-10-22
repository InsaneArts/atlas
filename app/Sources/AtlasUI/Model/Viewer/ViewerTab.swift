/// The main window's tabs, in the order the top row draws them.
package enum ViewerTab: Hashable, Sendable, CaseIterable {
  /// The helper's screen.
  case screen
  /// What was done in the helper.
  case activity
  /// The apps open in the helper.
  case apps
  /// The screenshots agents took.
  case screenshots
}
