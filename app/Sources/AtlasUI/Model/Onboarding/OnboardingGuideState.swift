/// The narrow panel beside System Settings while one permission's switch is turned on.
package struct OnboardingGuideState: Equatable, Sendable {
  /// Which permission of how many, and its name.
  package var eyebrow: String
  /// The first step's detail: where the switch is.
  package var settingsPath: String
  /// Where the second step, turning the switch on, stands.
  package var turnOn: StepState
  /// Where the third step, coming back, stands.
  package var comeBack: StepState
  /// What the panel waits for, or that the switch is on.
  package var pill: String
  /// Whether the switch is on, which ticks the pill.
  package var isOn: Bool

  /// Creates the guide panel's state.
  ///
  /// - Parameters:
  ///   - eyebrow: Which permission of how many, and its name.
  ///   - settingsPath: The first step's detail: where the switch is.
  ///   - turnOn: Where the second step, turning the switch on, stands.
  ///   - comeBack: Where the third step, coming back, stands.
  ///   - pill: What the panel waits for, or that the switch is on.
  ///   - isOn: Whether the switch is on, which ticks the pill.
  package init(
    eyebrow: String,
    settingsPath: String,
    turnOn: StepState,
    comeBack: StepState,
    pill: String,
    isOn: Bool
  ) {
    self.eyebrow = eyebrow
    self.settingsPath = settingsPath
    self.turnOn = turnOn
    self.comeBack = comeBack
    self.pill = pill
    self.isOn = isOn
  }
}
