import AtlasStatus
import AtlasUI

/// How each row of the status table looks, wherever it shows: the panel's heading, the main window's, and each
/// desktop's dot in the menu that picks one.
extension Mode.Kind {
  /// The word's and the mark's colours: blue while working, the link colour in control, orange when the person is
  /// needed, grey otherwise.
  var tone: StatusTone {
    switch self {
    case .working: .active
    case .control: .link
    case .needsYou, .stopped: .attention
    case .idle, .notReady, .loggedOut, .connecting, .inUseElsewhere: .quiet
    }
  }

  /// The mark: a breathing dot while working, a square once stopped, a small dot when not ready, else a dot.
  var mark: StatusMark {
    switch self {
    case .working: .pulsing
    case .stopped: .square
    case .notReady: .small
    case .idle, .control, .needsYou, .loggedOut, .connecting, .inUseElsewhere: .dot
    }
  }
}
