import AtlasKit
import AtlasUI

extension Mode.Tone {
  /// The tone in AtlasUI's own terms, for the views that paint it.
  var statusTone: StatusTone {
    switch self {
    case .ready: .ready
    case .active: .active
    case .attention: .attention
    case .quiet: .quiet
    }
  }
}
