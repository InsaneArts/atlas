import AtlasOnboarding
import AtlasUI

extension OnboardingStep {
  /// Where the step sits on the track at the bottom of the window: six segments on the person's side,
  /// where the form shares the choice's, and three on the helper's; none while blocked.
  var track: OnboardingFooterState.Track? {
    guard let place else { return nil }
    return OnboardingFooterState.Track(
      index: place.index,
      count: place.count,
      label: OnboardingWords.step(place.index, of: place.count)
    )
  }

  /// The step's segment and how many segments its side has; nil while a login is closed or setup is blocked.
  private var place: (index: Int, count: Int)? {
    switch self {
    case .welcome: (0, 6)
    case .chooseHelper, .createAccount: (1, 6)
    case .switchToHelper: (2, 6)
    case .finishing: (3, 6)
    case .agents: (4, 6)
    case .done: (5, 6)
    case .allow: (0, 3)
    case .permissions: (1, 3)
    case .helperDone: (2, 3)
    case .closeLeftovers, .blocked: nil
    }
  }
}
