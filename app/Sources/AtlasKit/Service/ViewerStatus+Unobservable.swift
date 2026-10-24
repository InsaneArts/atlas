extension ViewerStatus {
  /// Why the helper's desktop cannot be watched, which the app words.
  package enum Unobservable: Equatable, Sendable {
    /// The helper gives no answer: its overview's reason, nil before the service gives one.
    case silent(String?)
    /// What to do next, else the service's error, in the service's words; nil when it gives neither.
    case said(String?)
  }
}
