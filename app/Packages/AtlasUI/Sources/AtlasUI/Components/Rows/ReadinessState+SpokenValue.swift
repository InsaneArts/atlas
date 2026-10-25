extension ReadinessState {
  /// What VoiceOver says for a readiness check in this state.
  var spokenValue: String {
    switch self {
    case .ready: "Ready"
    case .notReady: "Not ready"
    case .unknown: "Unknown"
    }
  }
}
