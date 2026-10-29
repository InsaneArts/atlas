import Foundation

/// A moment of the Activity tab: one stream's work in one app, its actions at most two minutes apart on one day,
/// newest first. Actions in no app, such as a screenshot or a command, belong to the moment around them.
public struct ActivityMoment: Equatable, Sendable, Identifiable {
  /// 8: the most steps an open moment shows before "Show N more".
  public static let mostSteps = 8

  /// The moment's identity: its stream's and its oldest line's, which stays as newer lines join it.
  public var id: String
  /// Whose work it follows.
  public var stream: ActivityStream
  /// What it is titled by: its app, or Commands or Screenshots for a moment in no app.
  public var subject: Subject
  /// Its lines, newest first: every line of the moment, or those a filter and a search keep.
  public var runs: [ActivityRun]
  /// The first moment of the day it falls on, in the person's time zone.
  public var day: Date
  /// When its oldest line started, when known.
  public var startedAt: Date?
  /// When its newest line's newest row started, when known.
  public var endedAt: Date?

  /// Creates a moment.
  ///
  /// - Parameters:
  ///   - id: Its identity.
  ///   - stream: Whose work it follows.
  ///   - subject: What it is titled by.
  ///   - runs: Its lines, newest first.
  ///   - day: The first moment of its day.
  ///   - startedAt: When its oldest line started.
  ///   - endedAt: When its newest line's newest row started.
  init(
    id: String,
    stream: ActivityStream,
    subject: Subject,
    runs: [ActivityRun],
    day: Date,
    startedAt: Date?,
    endedAt: Date?
  ) {
    self.id = id
    self.stream = stream
    self.subject = subject
    self.runs = runs
    self.day = day
    self.startedAt = startedAt
    self.endedAt = endedAt
  }

  /// The app it acted in, when it acted in one.
  public var app: ActivityApp? {
    if case .app(let app) = subject { return app }
    return nil
  }

  /// How many actions its lines hold, each repeat counted.
  public var actions: Int {
    runs.reduce(0) { $0 + $1.count }
  }

  /// Whether one of its lines is still under way.
  public var isRunning: Bool {
    runs.contains { $0.entry.outcome == .running }
  }

  /// The lines an open moment shows, and how many more "Show N more" holds.
  ///
  /// - Parameter expanded: Whether the person asked for every line.
  /// - Returns: The newest ``mostSteps`` lines and the count of the rest, or every line and none when expanded.
  public func steps(expanded: Bool) -> (shown: [ActivityRun], more: Int) {
    guard !expanded, runs.count > Self.mostSteps else { return (runs, 0) }
    return (Array(runs.prefix(Self.mostSteps)), runs.count - Self.mostSteps)
  }
}
