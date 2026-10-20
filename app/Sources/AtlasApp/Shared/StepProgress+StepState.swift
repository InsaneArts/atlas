import AtlasKit
import AtlasUI

extension StepProgress {
  /// The progress in AtlasUI's own terms, for a step row.
  var stepState: StepState {
    switch self {
    case .waiting: .waiting
    case .current: .current
    case .done: .done
    }
  }
}
