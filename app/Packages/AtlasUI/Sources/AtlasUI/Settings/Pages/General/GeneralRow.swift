/// A row of the General page.
enum GeneralRow: CaseIterable, Identifiable {
  /// Open Atlas at login, a switch.
  case openAtLogin
  /// The menu bar's glyph, as segments.
  case menuBarIcon
  /// How often the screen updates, as segments.
  case picture
  /// Mute the helper, a switch.
  case muteHelper
  /// Notify me, a switch.
  case notify

  /// The rows of the page's Menu Bar section: how Atlas starts, and how it shows in the menu bar.
  static let menuBar: [GeneralRow] = [.openAtLogin, .menuBarIcon]

  /// Each row is listed once.
  var id: Self { self }
}
