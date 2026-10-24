extension OnboardingStep {
  /// Which way setup moves from one screen to the next, so the window slides its screens the way the person goes:
  /// back when the new screen comes earlier in setup, forward otherwise. A blocked screen has no place in the order,
  /// so going to one or leaving one moves forward, as does reading the same screen again.
  ///
  /// - Parameters:
  ///   - old: The screen shown until now.
  ///   - new: The screen shown next.
  /// - Returns: Back when `new` comes before `old`, else forward.
  public static func direction(from old: OnboardingStep, to new: OnboardingStep) -> StepDirection {
    guard let before = old.place, let after = new.place, after < before else { return .forward }
    return .back
  }

  /// Where the screen comes in setup, the person's screens before the helper's; nil for a blocked screen.
  private var place: Int? {
    switch self {
    case .welcome: 0
    case .chooseHelper: 1
    case .createAccount: 2
    case .switchToHelper: 3
    case .finishing: 4
    case .agents: 5
    case .done: 6
    case .allow: 7
    case .permissions: 8
    case .helperDone: 9
    case .blocked: nil
    }
  }
}
