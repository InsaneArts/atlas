import AtlasService
import AtlasUI

/// What Settings shares with the app's other parts, which only ``WindowCoordinator`` knows: the helper's status as the menu
/// bar last read it, Open at Login as the panel keeps it, the picture as the main window keeps it, switching to the
/// helper, and the setup window. Reached through closures, so Settings never holds another window, and a value the
/// panel or the main window also shows stays one value.
struct SettingsLinks {
  /// The helper's latest status, and whether Atlas waits for the service: before its first answer, and again after a
  /// reading that failed.
  var status: @MainActor () -> (status: ViewerStatus, connecting: Bool)
  /// Reads the helper's status again now.
  var readStatus: @MainActor () async -> Void
  /// Where Open at Login stands.
  var opensAtLogin: @MainActor () -> OpenAtLogin.State
  /// Turns Open at Login on or off.
  var setOpenAtLogin: @MainActor (Bool) async -> Void
  /// The picture choice in force: the main window's, else the rate saved.
  var picture: @MainActor () -> SettingsPicture
  /// Whether Paused can be chosen: only while the main window is open, since only it pauses.
  var canPause: @MainActor () -> Bool
  /// Makes a picture choice: the main window's when it is open, else the rate saved for the next one.
  var choosePicture: @MainActor (SettingsPicture) -> Void
  /// Locks the screen so the person can log in to the helper, returning what to tell them.
  var switchToHelper: @MainActor () async -> String
  /// Opens the setup window.
  var gettingStarted: @MainActor () -> Void
  /// Adds an app to the helper's accounts, by its bundle identifier and name, through the viewer service.
  var addAccount: @MainActor (_ app: String, _ name: String) async throws -> Void = { _, _ in }
  /// Takes an account off the helper's list, through the viewer service.
  var removeAccount: @MainActor (ViewerStatus.Account) async throws -> Void = { _ in }
  /// Sets how freely agents act in an account, through the viewer service.
  var setLevel: @MainActor (AccountLevel, ViewerStatus.Account) async throws -> Void = { _, _ in }
  /// Starts a held sign-in in the main window, which comes forward on the Screen: what it is for, the site and the
  /// browser. Returns why it couldn't start, or nil when it did.
  var startSignIn: @MainActor (SignInKind, String, String?) async -> String? = { _, _, _ in nil }
}
