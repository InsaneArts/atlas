import AtlasCommands

/// The Helper tab's words: the helper's line and what a check of it found.
extension SettingsWords {
  /// What the helper's account is, with its display when known.
  ///
  /// - Parameter display: Its display, such as "2560 × 1440", when known.
  /// - Returns: "The hidden helper login · 2560 × 1440".
  static func helperLine(display: String?) -> String {
    let line = "The hidden helper login"
    return display.map { "\(line) · \($0)" } ?? line
  }

  /// What Check Again's diagnosis found: ready, what to do first, or that it could not check.
  ///
  /// - Parameter answer: The diagnosis.
  /// - Returns: The line under the pane.
  static func diagnosis(_ answer: DoctorAnswer) -> String {
    switch answer {
    case .ready: "The helper is ready to act."
    case .needs(let steps): steps.isEmpty ? "The helper isn't ready to act." : steps.joined(separator: " ")
    case .failed(let message):
      message.isEmpty
        ? "Atlas couldn't check the helper." : "Atlas couldn't check the helper: \(Words.sentence(message))"
    }
  }
}
