/// Where the switch screen's three steps stand: log in to the helper, Atlas opening there, allow it.
public struct SwitchProgress: Equatable, Sendable {
  /// Log in to the helper.
  public let logIn: StepProgress
  /// Atlas opens in the helper by itself.
  public let openAtlas: StepProgress
  /// Allow Atlas there, twice.
  public let allow: StepProgress
}
