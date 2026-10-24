/// The main window's words that depend on its state: the top row's buttons and tooltips, the tabs, the empty
/// messages, Details' rows, the alerts, the menus and every notice.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts, and a list row's fixed name travels
/// with its value. Words the window shares with another are the app's shared ``Words`` or its ``HelperWords``.
enum ViewerWords {
  /// The window's title before the helper is known, which only the Window menu shows.
  static let windowTitle = "Atlas"

  /// The window's title once the helper is known, so the Window menu tells it from the setup window.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Atlas — “Atlas”".
  static func windowTitle(helper: String) -> String {
    "Atlas — “\(helper)”"
  }
}
