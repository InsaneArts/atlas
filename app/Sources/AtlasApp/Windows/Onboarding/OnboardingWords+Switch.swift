/// The switch screen's words: locking and logging in to the helper, Atlas opening there, and waiting for its
/// permissions and for the switch back.
extension OnboardingWords {
  /// Atlas answers in the helper, which still lacks a permission.
  static let waitingForPermissions = "Waiting for the two permissions…"
  /// The helper is ready, on screen.
  static let waitingForSwitchBack = "Waiting for you to switch back…"
  /// The switch to the helper did not lock the screen: a plain cause and the way to do it by hand.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "The screen didn’t lock. Press Control-Command-Q, then choose the Atlas account."
  static func lockFailed(_ helper: String) -> String {
    "The screen didn’t lock. Press Control-Command-Q, then choose \(named(helper))."
  }

  /// The switch screen's headline.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Now log in to the Atlas account."
  static func logIn(to helper: String) -> String {
    "Now log in to \(named(helper))."
  }

  /// The first step: log in to the helper.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Log in to the Atlas account"
  static func logInStep(to helper: String) -> String {
    "Log in to \(named(helper))"
  }

  /// The helper is on screen, and Atlas not open there yet.
  ///
  /// - Parameter helper: The helper as people know it.
  /// - Returns: "Waiting for Atlas to open in the Atlas account…"
  static func waitingForAtlas(in helper: String) -> String {
    "Waiting for Atlas to open in \(named(helper))…"
  }
}
