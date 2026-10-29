/// What the same stream did next after an action that did not work, as a problem's callout tells it.
public enum ActivityFollowUp: Equatable, Sendable {
  /// It tried the same action in the same app again, and that worked.
  case workedOnRetry
  /// It tried the same action in the same app again, and that did not work either.
  case failedAgain
  /// It went on to another action.
  ///
  /// - Parameter run: That action's line.
  case then(ActivityRun)

  /// What a line that did not work was followed by; none while its retry is still under way.
  ///
  /// - Parameters:
  ///   - next: The same stream's next newer line.
  ///   - problem: The line that did not work.
  init?(next: ActivityRun, after problem: ActivityRun) {
    guard Self.retries(next.entry, problem.entry) else {
      self = .then(next)
      return
    }
    switch next.entry.outcome {
    case .running: return nil
    case .worked: self = .workedOnRetry
    case .failed, .notSent: self = .failedAgain
    }
  }

  /// Whether a row does again what an earlier one did: the same verb, or the same kind for rows that name none, in
  /// the same app or in none.
  ///
  /// - Parameters:
  ///   - next: The newer row.
  ///   - problem: The earlier row.
  /// - Returns: Whether `next` is a retry of `problem`.
  static func retries(_ next: ActivityLog.Entry, _ problem: ActivityLog.Entry) -> Bool {
    let sameVerb =
      if let verb = next.verb, let earlier = problem.verb {
        verb == earlier
      } else {
        ActivityAction(next) == ActivityAction(problem)
      }
    guard sameVerb else { return false }
    switch (ActivityApp(next), ActivityApp(problem)) {
    case (nil, nil): return true
    case (let app?, let earlier?): return app.isSame(as: earlier)
    default: return false
    }
  }
}
