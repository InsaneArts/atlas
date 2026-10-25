extension CheckState {
  /// What VoiceOver says for a checklist line in this state.
  var spokenValue: String {
    switch self {
    case .done: "Passed"
    case .busy: "Checking"
    case .failed: "Failed"
    }
  }
}
