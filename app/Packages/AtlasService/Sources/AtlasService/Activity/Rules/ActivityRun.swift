import Foundation

/// One line of the Activity pane: a row of the log, or the same action done by the same actor several times one after
/// another, as one line with a count.
public struct ActivityRun: Equatable, Sendable, Identifiable {
  /// The run's earliest row: the line keeps its id, which a newer repeat never changes, and says what was done.
  public var entry: ActivityLog.Entry
  /// Every retained action's logical identity, for matching moments across readings.
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

  /// The line's identity: its earliest row's id.
  public var id: String { entry.id }

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
  }
}
