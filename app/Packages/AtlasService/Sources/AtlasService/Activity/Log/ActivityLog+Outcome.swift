extension ActivityLog {
  /// How something done in the helper went.
  public enum Outcome: Equatable, Sendable {
    /// It is still under way.
    case running
    /// It worked.
    case worked
    /// It failed, or the row does not say it worked.
    case failed
    /// It was refused before anything was sent, as when another app was in front: nothing to recover, and not a
    /// problem.
    case notSent

    /// The outcome of a row: one under way is running whatever `ok` says, since the service marks it ok so that apps
    /// reading only `ok` never show it as failed; and one that did not work but sent nothing was not sent, since the
    /// service keeps `ok` false on it so that older apps show it as they always did.
    ///
    /// - Parameters:
    ///   - started: Whether the row says it is under way.
    ///   - ok: Whether the row says it worked.
    ///   - nothingSent: Whether the row says it was refused before anything was sent.
    ///   - partial: Whether input was partly sent before the refusal.
    init(started: Bool, ok: Bool, nothingSent: Bool = false, partial: Bool = false) {
      if started {
        self = .running
      } else if ok {
        self = .worked
      } else {
        self = nothingSent && !partial ? .notSent : .failed
      }
    }
  }
}
