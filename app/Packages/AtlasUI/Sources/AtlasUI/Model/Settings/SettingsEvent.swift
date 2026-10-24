import CoreGraphics

/// What the person does in the Settings window, other than the General pane's values, which its binding carries, and
/// how tall a pane draws, so the window can fit it.
public enum SettingsEvent: Equatable, Sendable {
  /// Shows a pane.
  case selectTab(SettingsTab)
  /// A pane drew at this height, its insets included and the tab row not.
  case paneHeight(SettingsTab, CGFloat)
  /// Reads the helper's readiness again now.
  case checkAgain
  /// Asks to switch this Mac's screen to the helper.
  case askSwitch
  /// Asks to recover the helper.
  case askRecover
  /// Asks to log the helper out.
  case askLogOut
  /// Closes the logins deleted accounts left open; macOS asks to approve.
  case closeLeftovers
  /// Logs out the helper whose Atlas is silent, as root; macOS asks to approve.
  case closeHelper
  /// Connects the agent with this id.
  case connect(String)
  /// Disconnects every part of the agent with this id.
  case disconnect(String)
  /// Asks to forget the project in this folder.
  case askForget(String)
  /// Checks whether a newer Atlas is published.
  case checkForUpdates
  /// Asks to install the newer Atlas found.
  case askInstall
  /// Opens the setup window.
  case gettingStarted
  /// Shows Atlas in the Finder, so the person can move it to the Applications folder.
  case showAppInFinder
  /// Shows the settings file of the agent with this id in the Finder, so the person can mend it.
  case showAgentFile(String)
  /// The alert's button that goes ahead.
  case alertConfirm
  /// The alert's way out.
  case alertCancel
}
