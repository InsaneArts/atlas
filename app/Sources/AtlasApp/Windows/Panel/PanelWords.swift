/// The panel's words that depend on its state: its placeholders, its facts' names, what each action says while it
/// runs, when it ends and when it cannot run, and the menu bar item's label.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts, and a list row's fixed name travels
/// with its value. Words the panel shares with another window are AtlasKit's ``Words``.
enum PanelWords {
  /// Before the first capture.
  static let waitingForPicture = "Waiting for the helper's screen…"
  /// After a capture that gave no picture.
  static let pictureUnavailable = "The helper's screen is not available"
  /// The recovery card's detail when the helper gives no reason.
  static let recoverFirst = "Recover before anything else runs."
  /// The helper fact's name.
  static let helperFact = "Helper"
  /// The last-action fact's name.
  static let lastActionFact = "Last action"
  /// While Release Control runs.
  static let releasing = "Releasing control…"
  /// Once control is released.
  static let released = "Control released."
  /// While Switch to Helper… runs.
  static let switching = "Switching…"
  /// Once the switch ran.
  static let switched = "Switched."
  /// Log In… found no helper account.
  static let helperNotFound = "Atlas couldn't find the helper account. Open Getting Started to check it."
  /// While the helper logs out.
  static let loggingOut = "Logging out the helper…"
  /// Once the helper logged out.
  static let helperLoggedOut = "The helper is logged out."
  /// A log-out asked for while another action runs.
  static let waitForAction = "Another action is running. Try again once it ends."
  /// A log-out the helper's mode no longer allows.
  static let cannotLogOut = "The helper can't log out right now."

  /// The menu bar item's tooltip and VoiceOver label, which agree.
  ///
  /// - Parameter mode: The helper's mode in words.
  /// - Returns: "Atlas — Watching"
  static func statusItem(_ mode: String) -> String {
    "Atlas — \(mode)"
  }

  /// What to do once the screen is locked for the helper.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Log in to Atlas, then switch back."
  static func logInThenSwitchBack(to helper: String) -> String {
    "Log in to \(helper), then switch back."
  }
}
