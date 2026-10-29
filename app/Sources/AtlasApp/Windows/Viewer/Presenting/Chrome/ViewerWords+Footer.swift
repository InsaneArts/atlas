/// The footer's words: the main capsule in each state and its tooltips, and the text being typed. VoiceOver reads each
/// button by its menu item's exact title, and each tooltip is that title with its shortcut (``ShortcutWords``).
extension ViewerWords {
  /// The main capsule while watching.
  static let takeControl = "Take Control"
  /// The main capsule in control.
  static let giveBack = "Give Back"
  /// The main capsule while control is asked for, or waits for a fresh picture or the agent's step.
  static let connectingControl = "Connecting…"
  /// The main capsule while the helper needs the person.
  static let recoverButton = "Recover…"
  /// The main capsule's tooltip while the helper needs the person.
  static let recoverHelp = "Check the helper and start its mouse and keyboard again"

  /// The footer's line while the input method composes text not yet sent.
  ///
  /// - Parameter text: The text being composed.
  /// - Returns: "Typing こんにちは".
  static func typing(_ text: String) -> String {
    "Typing \(text)"
  }
}
