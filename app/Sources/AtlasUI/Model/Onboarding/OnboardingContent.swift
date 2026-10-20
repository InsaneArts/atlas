/// What fills the setup window.
package enum OnboardingContent: Equatable, Sendable {
  /// Nothing has been read yet.
  ///
  /// - Parameter problem: Why the first reading failed, if it did.
  case loading(problem: String?)
  /// The narrow panel beside System Settings while a switch is turned on.
  case guide(OnboardingGuideState)
  /// One setup screen.
  case screen(OnboardingScreen)
}
