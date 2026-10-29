/// A part of the sidebar's sessions, by what its sessions do, in the order they stand: those that need the person,
/// those working, those idle, then those that ended, folded under their heading until the person opens it.
public enum ViewerSessionSection: String, CaseIterable, Sendable {
  /// A session whose action may not have finished, so the helper needs the person.
  case needsYou
  /// Sessions acting on the helper or waiting their turn.
  case working
  /// Live sessions doing nothing now.
  case idle
  /// Sessions that ended, or went quiet long ago.
  case ended

  /// The heading's words.
  var title: String {
    switch self {
    case .needsYou: "Needs you"
    case .working: "Working"
    case .idle: "Idle"
    case .ended: "Ended"
    }
  }

  /// The heading's tone: attention over the sessions that need the person, quiet over the rest.
  var tone: StatusTone {
    self == .needsYou ? .attention : .quiet
  }
}
