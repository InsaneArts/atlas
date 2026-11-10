/// A row of the Activity page, in the order its sections list them.
enum ActivityRow: CaseIterable, Identifiable {
  /// Keep pictures, a switch.
  case picturesBefore
  /// How long pictures are kept, a popup.
  case keepFor
  /// The days a custom choice keeps pictures, a stepper.
  case keepDays
  /// How much typed text is kept, as segments.
  case typedText
  /// The characters a limit keeps, a stepper.
  case typedTextLimit
  /// How many pictures this Mac holds, with Delete All….
  case stored

  /// The rows of "Pictures of Agent Actions", each stepper only while its choice asks for a number.
  ///
  /// - Parameter state: The page's values.
  /// - Returns: The rows.
  static func pictures(_ state: SettingsGeneralState) -> [ActivityRow] {
    [.picturesBefore, .keepFor, .keepDays, .typedText, .typedTextLimit]
      .filter { row in
        switch row {
        case .keepDays: state.keepFor == .custom
        case .typedTextLimit: state.typedText == .limited
        default: true
        }
      }
  }

  /// Each row is listed once.
  var id: Self { self }
}
