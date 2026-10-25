/// A row of the General pane, in the order its sections list them.
enum GeneralRow: CaseIterable, Identifiable {
  /// Open Atlas at login, a switch.
  case openAtLogin
  /// How often the screen updates, a popup.
  case picture
  /// Notify me, a switch.
  case notify

  /// The rows of the pane's first section: how Atlas starts and shows the helper.
  static let start: [GeneralRow] = [.openAtLogin, .picture]
  /// The rows of "When Atlas needs you".
  static let needs: [GeneralRow] = [.notify]

  /// Each row is listed once.
  var id: Self { self }
}
