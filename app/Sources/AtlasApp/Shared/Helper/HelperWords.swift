import AtlasService

/// The words the helper's own commands say in every window that runs them: their menu items and how Recover's
/// refusals read. Words a single window says around them stay in that window's words.
/// `nonisolated`: constants, which a test's arguments read off the main actor too.
nonisolated enum HelperWords {
  /// The recovery alert's way out, which sets it aside.
  static let notNow = "Not Now"
  /// Once the helper logged out.
  static let helperLoggedOut = "The helper is logged out."
  /// The menu item that asks to recover the helper.
  static let recoverItem = "Recover…"
  /// The menu item that asks to restart the helper.
  static let restartItem = "Restart the Helper…"
  /// The menu item that asks to switch this Mac's screen to the helper.
  static let switchItem = "Switch to the Helper…"
  /// Switch to the Helper…'s tooltip, in the panel and Settings.
  static let switchHelp = "Lock your screen so you can log in to the helper"
  /// The menu item that asks to log the helper out.
  static let logOutItem = "Log Out the Helper…"
  /// Recover refused while a viewer or an agent's work holds the helper.
  static let recoverBusy = "The helper is in use. Try Recover again once it's free."
  /// Recover's input start refused because an earlier action's outcome is unknown.
  static let recoverNeedsRestart = "The last action's outcome is unknown. Recover again to restart the helper."
  /// A lost request for control could not be confirmed over: the service has not answered since.
  static let helperUnconfirmed = "Atlas couldn't confirm the helper is free. Try again once it answers."
  /// A command called off while it waited for control to be given back, before it ran.
  static let cancelled = "Cancelled before it ran."
  /// A command asked for while another runs, from this window or another.
  static let waitForAction = "Another action is running. Try again once it ends."

  /// Why Recover did not run, in plain words for the refusals the command line words for itself.
  ///
  /// - Parameter outcome: The refused command's outcome.
  /// - Returns: The plain words for a busy helper or an unknown outcome, else the command's own message.
  static func recoverRefused(_ outcome: Outcome) -> String {
    switch outcome.code {
    case "busy": recoverBusy
    case "uncertain": recoverNeedsRestart
    default: outcome.message
    }
  }
}
