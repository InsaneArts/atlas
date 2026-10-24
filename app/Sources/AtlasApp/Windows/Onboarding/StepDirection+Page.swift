import AtlasOnboarding
import AtlasUI

extension StepDirection {
  /// The way the window's screens slide for this move.
  var page: PageDirection {
    switch self {
    case .forward: .forward
    case .back: .back
    }
  }
}
