/// In the helper: the two permissions, and the button for the next one still off.
package struct OnboardingPermissionsScreenState: Equatable, Sendable {
  /// Accessibility, for input.
  package var accessibility: OnboardingPermissionItem
  /// Screen Recording, for pictures.
  package var screenRecording: OnboardingPermissionItem
  /// The first permission still off, whose button shows; nil once both are on.
  package var next: OnboardingPermissionItem?

  /// Creates the permissions screen's state.
  ///
  /// - Parameters:
  ///   - accessibility: Accessibility, for input.
  ///   - screenRecording: Screen Recording, for pictures.
  ///   - next: The first permission still off, whose button shows; nil once both are on.
  package init(
    accessibility: OnboardingPermissionItem,
    screenRecording: OnboardingPermissionItem,
    next: OnboardingPermissionItem? = nil
  ) {
    self.accessibility = accessibility
    self.screenRecording = screenRecording
    self.next = next
  }
}
