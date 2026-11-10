import AtlasCommands

/// The Helper page's words: how the helper stands, the helper's line, a login Atlas can close, and what a check of it
/// found.
extension SettingsWords {
  /// The Helper row's line while Atlas waits for the service.
  static let helperConnecting = "Connecting"
  /// The Helper row's line while the helper is logged out.
  static let helperLoggedOut = "Logged out"
  /// The Helper row's line while the helper needs recovering.
  static let helperNeedsYou = "Needs you"
  /// The Helper row's line while a readiness check fails.
  static let helperNeedsSetup = "Needs setup"
  /// The Helper row's line while the helper can act.
  static let helperReady = "Ready"

  /// A deleted account's login is still open.
  static let leftoverTitle = "A deleted account is still logged in"
  /// Why closing it matters.
  static let leftoverDetail = "Closes it, with your approval, so the next login doesn't stall"
  /// Its button.
  static let closeIt = "Close It…"
  /// The helper is logged in while Atlas there is silent.
  static let staleTitle = "The helper is still logged in"
  /// Why logging it out helps.
  static let staleDetail = "Atlas isn't answering there, so log it out to start it again"
  /// Its button.
  static let logOut = "Log Out…"
  /// While Atlas closes a login.
  static let closing = "Closing…"
  /// Once it closed.
  static let closed = "Closed. You can go on."

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
