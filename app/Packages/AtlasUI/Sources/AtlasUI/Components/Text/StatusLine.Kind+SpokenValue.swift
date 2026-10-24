extension StatusLine.Kind {
  /// What VoiceOver says before a status line's sentence, so a problem and a success are told apart by more than
  /// their colour.
  var spokenValue: String {
    switch self {
    case .working: "Working"
    case .problem: "Problem"
    case .done: "Done"
    }
  }
}
