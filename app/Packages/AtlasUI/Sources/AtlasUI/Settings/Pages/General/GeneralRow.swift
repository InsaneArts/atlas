/// A row of the General page.
enum GeneralRow: CaseIterable, Identifiable {
  /// Open Atlas at login, a switch.
  case openAtLogin
  /// How often the screen updates, as segments.
  case picture
  /// Mute the helper, a switch.
  case muteHelper
  /// Notify me, a switch.
  case notify

  /// Each row is listed once.
  var id: Self { self }
}
