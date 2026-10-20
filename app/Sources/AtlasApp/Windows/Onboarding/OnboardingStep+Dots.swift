import AtlasKit
import AtlasUI

extension OnboardingStep {
  /// Where the step sits among the dots at the bottom of the window: six on the person's side, where the form shares
  /// the choice's dot, and three on the helper's; none while blocked.
  var dots: OnboardingFooterState.Dots? {
    switch self {
    case .welcome: .init(index: 0, count: 6)
    case .chooseHelper, .createAccount: .init(index: 1, count: 6)
    case .switchToHelper: .init(index: 2, count: 6)
    case .finishing: .init(index: 3, count: 6)
    case .agents: .init(index: 4, count: 6)
    case .done: .init(index: 5, count: 6)
    case .allow: .init(index: 0, count: 3)
    case .permissions: .init(index: 1, count: 3)
    case .helperDone: .init(index: 2, count: 3)
    case .blocked: nil
    }
  }
}
