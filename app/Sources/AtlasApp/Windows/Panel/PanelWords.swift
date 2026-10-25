/// The panel's words that depend on its state: its lines, its main action and pause button, its empty picture, its
/// ⋯ menu and what its own actions say when they don't work.
///
/// The rule every window follows: fixed copy stays in AtlasUI's views; a presenter supplies only the words that
/// depend on state, interpolated, chosen by a condition or derived from facts. Words the panel shares with another
/// window are the app's shared ``Words`` or the helper commands' ``HelperWords``.
enum PanelWords {
  /// What VoiceOver calls the panel: Atlas, though its heading is the status.
  static let name = "Atlas"
  /// The panel's picture is paused while nothing acts: the line under Idle.
  static let pausedLine = "Picture paused · work in the helper goes on"
  /// The helper is logged out: the line beside the word.
  static let loggedOutLine = "Agents can't use the helper until you log in"
  /// The helper isn't answering, when the service gives no reason: the line beside the word.
  static let notAnswering = "Atlas isn't answering in the helper"
  /// The empty picture while the helper is logged out.
  static let loggedOutPicture = "The helper is logged out"
  /// The empty picture while the helper isn't ready.
  static let notReadyPicture = "Recover checks the helper and starts it again"
  /// The empty stage's words while the helper isn't ready for a reason Recover can't fix, such as a service that
  /// isn't answering.
  static let notReadyWaitPicture = "Atlas checks again by itself. Settings shows what the helper needs."
  /// The main action that opens the main window.
  static let openAtlas = "Open Atlas"
  /// Open Atlas's tooltip.
  static let openAtlasHelp = "Open Atlas's window"
  /// The main action while Atlas's window holds control.
  static let giveBack = "Give Back"
  /// Give Back's tooltip.
  static let giveBackHelp = "Give control back to the agent"
  /// The main action while the helper isn't ready for a reason Recover can't fix: the Settings window, which shows
  /// what the helper needs.
  static let openSettings = "Open Settings"
  /// Open Settings' tooltip.
  static let openSettingsHelp = "See what the helper needs in Settings"
  /// Recover…'s tooltip.
  static let recoverHelp = "Check the helper and start its mouse and keyboard again"
  /// The ⋯ menu's item that opens the main window.
  static let openAtlasItem = "Open Atlas"
  /// Switch found no helper account.
  static let helperNotFound = "Atlas couldn't find the helper account. Open Getting Started to check it."

  /// The menu bar item's tooltip and VoiceOver label, which agree.
  ///
  /// - Parameter mode: The helper's mode in words.
  /// - Returns: "Atlas — Idle"
  static func statusItem(_ mode: String) -> String {
    "Atlas — \(mode)"
  }

  /// Why agents can't use the helper yet, beside Not ready: the service's reason, else that it isn't answering.
  ///
  /// - Parameter reason: The service's reason; nil before it gives one, or empty, for none.
  /// - Returns: The reason, or "Atlas isn't answering in the helper".
  static func notReadyLine(_ reason: String?) -> String {
    guard let reason, !reason.isEmpty else { return notAnswering }
    return reason
  }

  /// What to do once the screen is locked for the helper.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Log in to Atlas, then switch back."
  static func logInThenSwitchBack(to helper: String) -> String {
    "Log in to \(helper), then switch back."
  }
}
