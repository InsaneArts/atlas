/// Only the contents a moment can reveal, retained while its disclosure folds.
struct ActivityMomentContent: Equatable {
  /// Its typed steps.
  var steps: [ViewerActivityStep]
  /// Its optional Show more control.
  var more: ViewerMoreSteps?

  /// Whether the disclosure has anything to reveal.
  var isEmpty: Bool { steps.isEmpty && more == nil }
}
