extension ActivityLog {
  /// How something done in the helper went.
  package enum Outcome: Equatable, Sendable {
    /// It is still under way.
    case running
    /// It worked.
    case worked
    /// It failed, or the row does not say it worked.
    case failed

    /// The outcome of a row: one under way is running whatever `ok` says, since the service marks it ok so that apps
    /// reading only `ok` never show it as failed.
    ///
    /// - Parameters:
    ///   - started: Whether the row says it is under way.
    ///   - ok: Whether the row says it worked.
    package init(started: Bool, ok: Bool) {
      if started {
        self = .running
      } else {
        self = ok ? .worked : .failed
      }
    }
  }
}
