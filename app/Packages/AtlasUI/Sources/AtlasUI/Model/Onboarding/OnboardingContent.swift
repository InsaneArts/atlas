/// What fills the setup window.
public enum OnboardingContent: Equatable, Sendable {
  /// Nothing has been read yet.
  ///
  /// - Parameter problem: Why the first reading failed, if it did.
  case loading(problem: String?)
  /// The compact panel beside System Settings while a permission is allowed.
  case permissionPanel(OnboardingPermissionPanelState)
  /// One setup screen.
  case screen(OnboardingScreen)
}
