extension ViewerStatus {
  /// Why the helper's desktop cannot be watched now; nil when it can. A silent helper is explained by its overview's
  /// reason, never the commands its next steps name; one that answers says what to do next, else the service's error.
  public var unobservable: Unobservable? {
    guard !canObserve else { return nil }
    if !overview.mode.answers, error == nil { return .silent(overview.reason) }
    return .said(nextSteps.isEmpty ? error : nextSteps.joined(separator: " "))
  }
}
