import CoreGraphics

/// What the person does in the Settings window, other than the General pane's values, which its binding carries, and
/// how tall a pane draws, so the window can fit it.
public enum SettingsEvent: Equatable, Sendable {
  /// Shows a pane.
  case selectTab(SettingsTab)
  /// A pane drew at this height, its insets included and the tab row not (v3 G2).
  case paneHeight(SettingsTab, CGFloat)
  /// Reads the helper's readiness again now.
  case checkAgain
  /// Asks to switch this Mac's screen to the helper.
  case askSwitch
  /// Asks to recover the helper.
  case askRecover
  /// Asks to log the helper out.
  case askLogOut
  /// Connects the agent with this id.
  case connect(String)
  /// Asks to forget the project in this folder.
  case askForget(String)
  /// Checks whether a newer Atlas is published.
  case checkForUpdates
  /// Asks to install the newer Atlas found.
  case askInstall
  /// Opens the setup window.
  case gettingStarted
  /// The alert's button that goes ahead.
  case alertConfirm
  /// The alert's way out.
  case alertCancel
}
