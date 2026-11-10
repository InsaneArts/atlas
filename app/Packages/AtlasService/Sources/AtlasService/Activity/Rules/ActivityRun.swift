import Foundation

/// One line of the Activity pane: a row of the log, or the same action done by the same actor three or more times one
/// after another, folded into one line with a count that keeps each of its rows.
public struct ActivityRun: Equatable, Sendable, Identifiable {
  /// The run's earliest row: the line keeps its id, which a newer repeat never changes, and says what was done.
  public var entry: ActivityLog.Entry
  /// Every retained action's logical identity, for matching app runs across readings.
  var logicalIDs: Set<String>
  /// When the run started: its earliest row's start, when known.
  public var startedAt: Date?
  /// When the run's newest row started, when known: its start for a row on its own.
  public var endedAt: Date?
  /// How many rows the run holds; 1 for a row on its own.
  public var count: Int
  /// How many actions its rows carried, including input already aggregated by the service.
  public internal(set) var actions: Int
  /// The capture a screenshot row took, by its file name, when the screenshots list it.
  public var shot: String?
  /// Every row it holds, oldest first: the row itself for a line on its own.
  public internal(set) var members: [ActivityLog.Entry]

  /// The line's identity: its earliest row's id.
  public var id: String { entry.id }

  /// Its first row that kept a picture taken right before it: the line shows that picture, marked as that row. A
  /// repeat whose picture was not taken leaves it to the next.
  public var pictured: ActivityLog.Entry? {
    members.first { $0.before != nil }
  }

  /// The picture taken right before the first of its rows that kept one, by its file name.
  public var firstBefore: String? { pictured?.before }

  /// Creates a line.
  ///
  /// - Parameters:
  ///   - entry: The run's earliest row.
  ///   - startedAt: When the run started.
  ///   - count: How many rows it holds.
  ///   - shot: The capture a screenshot row took, by its file name.
  ///   - endedAt: When its newest row started; its start when nil, as for a row on its own.
  init(entry: ActivityLog.Entry, startedAt: Date?, count: Int = 1, shot: String? = nil, endedAt: Date? = nil) {
    self.entry = entry
    logicalIDs = [entry.logicalId]
    self.startedAt = startedAt
    self.endedAt = endedAt ?? startedAt
    self.count = count
    actions = ActivityGrouping.actionCount(entry.count)
    self.shot = shot
    members = [entry]
  }
}
