/// In the helper: the two permissions, each with where it stands; the next one to allow is the footer's button.
public struct OnboardingPermissionsScreenState: Equatable, Sendable {
  /// Accessibility, for input.
  public var accessibility: OnboardingPermissionItem
  /// Screen Recording, for pictures.
  public var screenRecording: OnboardingPermissionItem

  /// Creates the permissions screen's state.
  ///
  /// - Parameters:
  ///   - accessibility: Accessibility, for input.
  ///   - screenRecording: Screen Recording, for pictures.
  public init(accessibility: OnboardingPermissionItem, screenRecording: OnboardingPermissionItem) {
    self.accessibility = accessibility
    self.screenRecording = screenRecording
  }
}
