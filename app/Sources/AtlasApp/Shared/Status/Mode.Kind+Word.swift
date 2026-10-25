import AtlasStatus

extension Mode.Kind {
  /// The row's own word, when the service gives none.
  var word: String {
    switch self {
    case .idle: "Idle"
    case .working: "Working"
    case .control: "You have control"
    case .needsYou: "Needs you"
    case .stopped: "Stopped"
    case .notReady: "Not ready"
    case .loggedOut: "Logged out"
    case .connecting: "Connecting"
    case .inUseElsewhere: Words.inUseElsewhere
    }
  }
}
