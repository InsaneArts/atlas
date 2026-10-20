extension ViewerStatus {
  /// Why the helper's desktop cannot be watched now, in words; nil when it can. A silent helper is explained in the
  /// app's words, its overview's reason, never the commands its next steps name; one that answers says what to do
  /// next, else the service's error.
  package var unobservableReason: String? {
    guard !canObserve else { return nil }
    if !overview.mode.answers, error == nil { return overview.reason }
    let steps = nextSteps.isEmpty ? [error ?? "Helper unavailable"] : nextSteps
    return steps.joined(separator: " ")
  }
}
