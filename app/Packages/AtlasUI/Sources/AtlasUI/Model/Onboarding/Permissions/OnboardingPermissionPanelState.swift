/// The compact panel beside System Settings while one permission is allowed: what to do there, and both permissions.
public struct OnboardingPermissionPanelState: Equatable, Sendable {
  /// Which permission of how many, and its name.
  public var label: String
  /// What to do in System Settings: drag Atlas into the right list.
  public var instruction: String
  /// For Atlas listed already but off, or for anyone who can't drag: turning its switch on.
  public var alreadyListed: String
  /// Accessibility, for input.
  public var accessibility: OnboardingPermissionItem
  /// Screen Recording, for pictures.
  public var screenRecording: OnboardingPermissionItem
  /// What the panel waits for, or that the permission is on.
  public var pill: String
  /// Whether the permission guided now is on, which ticks the pill.
  public var isOn: Bool

  /// Creates the panel's state.
  ///
  /// - Parameters:
  ///   - label: Which permission of how many, and its name.
  ///   - instruction: What to do in System Settings: drag Atlas into the right list.
  ///   - alreadyListed: For Atlas listed already but off, or for anyone who can't drag: turning its switch on.
  ///   - accessibility: Accessibility, for input.
  ///   - screenRecording: Screen Recording, for pictures.
  ///   - pill: What the panel waits for, or that the permission is on.
  ///   - isOn: Whether the permission guided now is on, which ticks the pill.
  public init(
    label: String,
    instruction: String,
    alreadyListed: String,
    accessibility: OnboardingPermissionItem,
    screenRecording: OnboardingPermissionItem,
    pill: String,
    isOn: Bool
  ) {
    self.label = label
    self.instruction = instruction
    self.alreadyListed = alreadyListed
    self.accessibility = accessibility
    self.screenRecording = screenRecording
    self.pill = pill
    self.isOn = isOn
  }
}
