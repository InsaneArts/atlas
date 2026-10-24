import AtlasUI

/// The short words that say where a check or a permission stands, one vocabulary for every window, each capitalised
/// the same way and painted in one tone wherever it shows, so "Allowed" never turns into a grey "allowed" on another
/// screen. Each window's presenter takes its words from here.
enum StatusWord: CaseIterable {
  /// A permission that is on.
  case allowed
  /// A step whose turn has not come yet.
  case next
  /// A step that waits for the person to act, such as in System Settings.
  case waitingForYou
  /// A check under way.
  case checking
  /// A check that passed.
  case passed
  /// A check that failed.
  case failed
  /// A permission that was allowed and is off again.
  case turnedOff

  /// The word.
  var text: String {
    switch self {
    case .allowed: "Allowed"
    case .next: "Next"
    case .waitingForYou: "Waiting for you…"
    case .checking: "Checking…"
    case .passed: "Passed"
    case .failed: "Failed"
    case .turnedOff: "Turned off"
    }
  }

  /// Its tone: green once it worked, orange for a real problem, quiet while it waits or runs.
  var tone: StatusTone {
    switch self {
    case .allowed, .passed: .ready
    case .failed, .turnedOff: .attention
    case .next, .waitingForYou, .checking: .quiet
    }
  }
}
