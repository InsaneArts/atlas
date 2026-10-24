/// The main window's words that depend on its state: the top row's buttons and tooltips, the tabs, the empty
/// messages, Details' rows, the alerts, the menus and every notice.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts, and a list row's fixed name travels
/// with its value. Words the window shares with another are the app's shared ``Words`` or its ``HelperWords``.
enum ViewerWords {
  /// The helper's name when the status gives none.
  static let helper = "Helper"
  /// The window's title, which only the Window menu shows.
  static let windowTitle = "Atlas"
}
