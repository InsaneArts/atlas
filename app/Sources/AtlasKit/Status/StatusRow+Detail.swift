extension StatusRow {
  /// What a row found.
  package enum Detail: Equatable, Sendable {
    /// The check passed.
    case passed
    /// The check cannot be observed yet.
    case notKnown
    /// The check failed; its step may name a command, which the app never shows.
    case failed(ViewerStatus.Check)
    /// The check failed while the helper gives no answer, so the overview's reason stands for it: the service's
    /// reason, nil before it gives one.
    case helperSilent(String?)
    /// The native dialogs worked, this many seconds ago when known.
    case worked(ageSeconds: Double?)
    /// The native dialogs did not work, this many seconds ago when known.
    case didNotWork(ageSeconds: Double?)
    /// The native dialogs were not checked in this session yet.
    case notChecked
  }
}
