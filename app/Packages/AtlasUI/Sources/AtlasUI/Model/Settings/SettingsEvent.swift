/// What the person does in Settings, other than the General pane values carried by its binding.
public enum SettingsEvent: Equatable, Sendable {
  /// Shows a pane.
  case selectTab(SettingsTab)
  /// Opens a root or detail page without running a command.
  case navigate(SettingsDestination)
  /// Edits only the transient local query.
  case search(String)
  /// Focuses the search field through Find's existing responder action.
  case find
  /// Moves the selected local result one step, without opening it.
  case moveSearchSelection(Int)
  /// Selects a current result when keyboard focus reaches its row.
  case selectSearchResult(SettingsSearchTarget)
  /// Opens a current match's location without activating its control.
  case openSearchResult(SettingsSearchTarget)
  /// Opens the selected match, consuming Return in the field.
  case openSelectedSearchResult
  /// Changes only the startup preference through its existing writer.
  case setOpenAtLogin(Bool)
  /// Changes only the screen cadence through its existing writer.
  case choosePicture(SettingsPicture)
  /// Changes only the notification preference through its existing writer.
  case setNotify(Bool)
  /// Delegates to the Help menu's existing diagnostics operation.
  case copyDiagnostics
  /// Returns to the previous destination.
  case back
  /// Returns to the next destination.
  case forward
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
