import AtlasStatus

extension SessionFacts.Input {
  /// Whether input is ready, as the helper's facts list it: "Ready", "Not ready" or "Not answering".
  var word: String {
    switch self {
    case .ready: "Ready"
    case .notReady: "Not ready"
    case .notAnswering: "Not answering"
    }
  }
}
