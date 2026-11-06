/// A row of the General pane, in the order its sections list them.
enum GeneralRow: CaseIterable, Identifiable {
  /// Open Atlas at login, a switch.
  case openAtLogin
  /// How often the screen updates, a popup.
  case picture
  /// Mute the helper, a switch.
  case muteHelper
  /// Pictures of agent actions, a switch.
  case picturesBefore
  /// How long pictures are kept, a popup.
  case keepFor
  /// The days a custom choice keeps pictures, a stepper.
  case keepDays
  /// How much typed text is kept, a popup.
  case typedText
  /// The characters a limit keeps, a stepper.
  case typedTextLimit
  /// How many pictures this Mac holds, with Delete All….
  case stored
  /// Notify me, a switch.
  case notify

  /// The rows of the pane's first section: how Atlas starts, and how the helper shows and sounds.
  static let start: [GeneralRow] = [.openAtLogin, .picture, .muteHelper]
  /// The rows of "Activity": pictures of agent actions, each stepper only while its popup asks for a number.
  ///
  /// - Parameter state: The pane.
  /// - Returns: The rows.
  static func activity(_ state: SettingsGeneralState) -> [GeneralRow] {
    [.picturesBefore, .keepFor, .keepDays, .typedText, .typedTextLimit, .stored]
      .filter { row in
        switch row {
        case .keepDays: state.keepFor == .custom
        case .typedTextLimit: state.typedText == .limited
        default: true
        }
      }
  }
  /// The rows of "When Atlas needs you".
  static let needs: [GeneralRow] = [.notify]

  /// Each row is listed once.
  var id: Self { self }
}
