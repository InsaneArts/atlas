extension StepState {
  /// What VoiceOver says for a step in this state.
  var spokenValue: String {
    switch self {
    case .done: "Done"
    case .current: "Now"
    case .waiting: "Later"
    }
  }
}
