import AtlasService
import AtlasUI

/// How a session's state looks beside its word, in the panel and the main window alike.
extension HostSessionState {
  /// The word's and the mark's tone: blue while it acts, orange while it needs the person, grey otherwise.
  var tone: StatusTone {
    switch self {
    case .acting: .active
    case .needsYou: .attention
    case .waiting, .idle, .ended: .quiet
    }
  }

  /// The mark: a ring while it waits its turn, a small dot once it ended, else a dot.
  var mark: StatusMark {
    switch self {
    case .waiting: .ring
    case .ended: .small
    case .acting, .needsYou, .idle: .dot
    }
  }
}
