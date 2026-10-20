import AtlasKit
import AtlasUI

extension StatusRow.State {
  /// The state in AtlasUI's own terms, for a readiness row.
  var readiness: ReadinessState {
    switch self {
    case .ready: .ready
    case .unknown: .unknown
    case .bad: .notReady
    }
  }
}
