/// The switch screen's words: locking and logging in to the helper, Atlas opening there, and waiting for its
/// permissions and for the switch back.
extension OnboardingWords {
  /// Atlas answers in the helper, which still lacks a permission.
  static let waitingForPermissions = "Waiting for the two permissions…"
  /// The helper is ready, on screen.
  static let waitingForSwitchBack = "Waiting for you to switch back…"
  /// The helper is logged in but not on screen, and Atlas not set up there yet.
  static let lockThenAllow = "Next: Lock and Switch, then allow Atlas there."

  /// The switch screen's headline.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Now log in to the Atlas account."
  static func logIn(to helper: String) -> String {
    "Now log in to \(named(helper))."
  }

  /// Where to choose the helper.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Choose the Atlas account on the lock screen"
  static func chooseOnLockScreen(_ helper: String) -> String {
    "Choose \(named(helper)) on the lock screen"
  }

  /// The helper has not logged in yet.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Next: Lock and Switch, then log in to the Atlas account."
  static func lockThenLogIn(to helper: String) -> String {
    "Next: Lock and Switch, then log in to \(named(helper))."
  }

  /// The helper is on screen, and Atlas not open there yet.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Waiting for Atlas to open in the Atlas account…"
  static func waitingForAtlas(in helper: String) -> String {
    "Waiting for Atlas to open in \(named(helper))…"
  }
}
