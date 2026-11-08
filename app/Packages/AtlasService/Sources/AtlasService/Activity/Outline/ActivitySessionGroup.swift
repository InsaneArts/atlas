import Foundation

/// A session of the Activity outline: one stream's runs on one day, newest first, and what they held.
public struct ActivitySessionGroup: Equatable, Sendable, Identifiable {
  /// Its identity: its stream's and its day's.
  public var id: String
  /// Whose work it was.
  public var stream: ActivityStream
  /// The first moment of its day.
  public var day: Date
  /// Its runs, newest first.
  public var runs: [ActivityMoment]

  /// Creates a session's group with its newest run.
  ///
  /// - Parameter run: The run.
  init(_ run: ActivityMoment) {
    id = "\(run.stream.id)|\(Int(run.day.timeIntervalSince1970))"
    stream = run.stream
    day = run.day
    runs = [run]
  }

  /// When its oldest run started, when known.
  public var startedAt: Date? {
    runs.compactMap(\.startedAt)
      .min()
  }

  /// When its newest run ended, when known.
  public var endedAt: Date? {
    runs.compactMap(\.endedAt)
      .max()
  }

  /// Whether one of its runs is under way.
  public var isRunning: Bool {
    runs.contains(where: \.isRunning)
  }

  /// Its screenshots that kept a capture, newest first: the pictures it shows while folded.
  public var shots: [ActivityRun] {
    runs.flatMap(\.shots)
  }

  /// What kinds of action its runs held and in which apps.
  public var tally: ActivityTally {
    ActivityTally(runs.flatMap(\.runs))
  }

  /// The sessions the runs form: each stream's runs on one day, in the order their newest run comes.
  ///
  /// - Parameter runs: The runs, newest first.
  /// - Returns: The sessions, newest first.
  public static func grouped(_ runs: [ActivityMoment]) -> [ActivitySessionGroup] {
    var groups: [ActivitySessionGroup] = []
    var places: [String: Int] = [:]
    for run in runs {
      let group = ActivitySessionGroup(run)
      if let place = places[group.id] {
        groups[place].runs.append(run)
      } else {
        places[group.id] = groups.count
        groups.append(group)
      }
    }
    return groups
  }
}
