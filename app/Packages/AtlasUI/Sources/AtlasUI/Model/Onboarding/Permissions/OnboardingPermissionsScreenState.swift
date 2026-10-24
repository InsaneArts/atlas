/// In the helper: the two permissions, the next one still off with its button.
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
