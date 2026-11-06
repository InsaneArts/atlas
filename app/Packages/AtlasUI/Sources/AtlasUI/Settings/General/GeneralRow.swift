/// A row of the General pane, in the order its sections list them.
enum GeneralRow: CaseIterable, Identifiable {
  /// Open Atlas at login, a switch.
  case openAtLogin
  /// How often the screen updates, a popup.
  case picture
  /// Mute the helper, a switch.
  case muteHelper
  /// Notify me, a switch.
  case notify

  /// The rows of the pane's first section: how Atlas starts, and how the helper shows and sounds.
  static let start: [GeneralRow] = [.openAtLogin, .picture, .muteHelper]
  /// The rows of "When Atlas needs you".
  static let needs: [GeneralRow] = [.notify]

  /// Each row is listed once.
  var id: Self { self }
}
