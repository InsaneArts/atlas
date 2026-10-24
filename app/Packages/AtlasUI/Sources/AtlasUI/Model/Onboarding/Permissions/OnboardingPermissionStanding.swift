/// Where a permission stands, in the same words on the permissions screen's rows and in the panel beside System
/// Settings: a word for each situation, never a verdict for a permission whose turn has not come.
public enum OnboardingPermissionStanding: Hashable, Sendable {
  /// On: a green check and its word, Allowed.
  case allowed(String)
  /// Its turn, with System Settings open on its list: the person acts there, so its word, Waiting for you…, has no
  /// spinner.
  case waiting(String)
  /// Its turn, its list not opened yet: the footer's button offers it, so the row says nothing.
  case yourTurn
  /// Its turn has not come yet: a quiet Next, the row dimmed.
  case next(String)
  /// Allowed once and off again, the one real problem: Turned off, in orange.
  case turnedOff(String)
  /// Not read yet, as Atlas's job restarts: a spinner and Checking…, after a moment, never shown as off.
  case checking(String)

  /// Whether the row it ends is dimmed, its turn not come yet.
  public var dimsRow: Bool {
    if case .next = self { return true }
    return false
  }
}
