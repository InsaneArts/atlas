import Foundation

extension ActivityMoments {
  /// A moment while its lines are gathered, oldest last.
  struct Forming {
    /// Whose work it follows.
    var stream: ActivityStream
    /// The first moment of its day.
    var day: Date
    /// Its lines so far, newest first.
    var runs: [ActivityRun]
    /// The app its lines acted in, once one names it.
    var app: ActivityApp?
    /// When its oldest line so far started, when known.
    var startedAt: Date?
    /// When its newest line's newest row started, when known.
    var endedAt: Date?

    /// Opens a moment with its newest line.
    ///
    /// - Parameters:
    ///   - stream: Whose work it follows.
    ///   - day: The first moment of the line's day.
    ///   - run: The line.
    ///   - app: The app the line acted in.
    init(stream: ActivityStream, day: Date, run: ActivityRun, app: ActivityApp?) {
      self.stream = stream
      self.day = day
      runs = [run]
      self.app = app
      startedAt = run.startedAt
      endedAt = run.endedAt
    }

    /// Whether an older line joins the moment: on the same day, at most ``ActivityMoments/momentGap`` before its
    /// oldest line, and in the same app, in none, or the moment is in none yet. A line or a moment with no time is
    /// never too far.
    ///
    /// - Parameters:
    ///   - run: The line.
    ///   - app: The app it acted in.
    ///   - day: The first moment of its day.
    /// - Returns: Whether it joins.
    func takes(_ run: ActivityRun, app: ActivityApp?, day: Date) -> Bool {
      guard day == self.day else { return false }
      if let startedAt, let ended = run.endedAt, startedAt.timeIntervalSince(ended) > ActivityMoments.momentGap {
        return false
      }
      guard let app, let own = self.app else { return true }
      return own.isSame(as: app)
    }

    /// Adds an older line, which a moment in no app takes its app from.
    ///
    /// - Parameters:
    ///   - run: The line.
    ///   - app: The app it acted in.
    mutating func join(_ run: ActivityRun, app: ActivityApp?) {
      runs.append(run)
      if let app { self.app = self.app?.filled(from: app) ?? app }
      if let started = run.startedAt { startedAt = min(startedAt ?? started, started) }
      if let ended = run.endedAt { endedAt = max(endedAt ?? ended, ended) }
    }

    /// The moment, its identity its stream's and its oldest line's.
    var moment: ActivityMoment {
      ActivityMoment(
        id: "\(stream.id)|\(runs.last?.id ?? "")",
        stream: stream,
        subject: app.map(ActivityMoment.Subject.app) ?? .appless(runs),
        runs: runs,
        day: day,
        startedAt: startedAt,
        endedAt: endedAt
      )
    }
  }
}
