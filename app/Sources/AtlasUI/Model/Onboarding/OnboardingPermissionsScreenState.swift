/// In the helper: the two permissions, the next one still off with its button.
package struct OnboardingPermissionsScreenState: Equatable, Sendable {
  /// Accessibility, for input.
  package var accessibility: OnboardingPermissionItem
  /// Screen Recording, for pictures.
  package var screenRecording: OnboardingPermissionItem

  /// Creates the permissions screen's state.
  ///
  /// - Parameters:
  ///   - accessibility: Accessibility, for input.
  ///   - screenRecording: Screen Recording, for pictures.
  package init(accessibility: OnboardingPermissionItem, screenRecording: OnboardingPermissionItem) {
    self.accessibility = accessibility
    self.screenRecording = screenRecording
  }
}
