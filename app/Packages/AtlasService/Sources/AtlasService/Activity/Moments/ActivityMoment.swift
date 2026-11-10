import Foundation

/// A run of the Activity outline: one stream's work in one app, its actions at most two minutes apart on one day,
/// newest first. Actions in no app, such as a screenshot or a command, belong to the run around them, and neighbouring
/// ones in no app form one run.
public struct ActivityMoment: Equatable, Sendable, Identifiable {
  /// The run's identity, retained across readings with overlapping actions, even when its oldest row leaves.
  public var id: String
  /// Whose work it follows.
  public var stream: ActivityStream
  /// What it is titled by: its app, or Commands or Screenshots for a run in no app.
  public var subject: Subject
  /// Its lines, newest first: every line of the run, or those a filter and a search keep.
  public var runs: [ActivityRun]
  /// The first moment of the day it falls on, in the person's time zone.
  public var day: Date
  /// When its oldest line started, when known.
  public var startedAt: Date?
  /// When its newest line's newest row started, when known.
  public var endedAt: Date?

  /// Creates a run.
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

  /// How many actions its lines hold, including repeats and aggregated personal input.
  public var actions: Int {
    runs.reduce(0) { ActivityGrouping.addingActions($1.actions, to: $0) }
  }

  /// Whether one of its lines is still under way.
  public var isRunning: Bool {
    runs.contains { $0.entry.outcome == .running }
  }

  /// Its lines oldest first, as an opened run reads, like a story.
  public var story: [ActivityRun] {
    runs.reversed()
  }

  /// Its screenshots that kept a capture, newest first: the pictures a folded run shows.
  public var shots: [ActivityRun] {
    runs.filter { $0.shot != nil }
  }

  /// Its lines that did not work, newest first: the problems a folded run shows.
  public var problems: [ActivityRun] {
    runs.filter { $0.entry.outcome == .failed }
  }

  /// What kinds of action it held and in which apps.
  public var tally: ActivityTally {
    ActivityTally(runs)
  }
}
