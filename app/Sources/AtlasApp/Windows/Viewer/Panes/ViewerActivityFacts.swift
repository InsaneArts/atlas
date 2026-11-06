import AtlasService
import AtlasUI
import Foundation

/// What the Activity pane last read, and what the person asked of it: the filter, the moments they opened or folded,
/// and those they asked to list every step of, each kept for the window's life, and the action shown large or the task
/// replayed with the pictures read for them. The sidebar's search narrows it.
struct ViewerActivityFacts {
  /// The log, newest first; nil before the first reading.
  var log: ActivityLog?
  /// The model's prepared reading, copied as values with this snapshot.
  var preparation: ViewerActivityPreparation?
  /// This reading's moments, whose identities were reconciled at ingestion.
  var moments: [ActivityMoment] { preparation?.moments.moments ?? [] }
  /// When the log was read, which each row's age counts back from.
  var readAt: Date?
  /// Why the last reading failed, while it has; nil once one worked.
  var failure: String?
  /// The filter chosen.
  var filter = ViewerActivityFilter.all
  /// The person's own choice for a moment, by its id: true to open it, false to fold it. A choice, not a flip of the
  /// default, so a moment the person opened stays open as newer actions make it the newest.
  var choices: [String: Bool] = [:]
  /// The moments the person asked to list every step of, by their ids.
  var expanded: Set<String> = []
  /// The action shown large over the window; nil shows none.
  var action: ViewerActionFocus?
  /// The task replayed over the window; nil shows none.
  var replay: ViewerReplayFocus?
  /// The pictures of agent actions read as taken, for the action shown large and the replay, by file name.
  var pictures: [String: DesktopPicture] = [:]
  /// The squares that changed between two pictures, as shares of them, by ``changesKey(before:after:)``.
  var changes: [String: [CGRect]] = [:]

  /// How the squares that changed between two pictures are kept.
  ///
  /// - Parameters:
  ///   - before: The picture before, by file name.
  ///   - after: The picture after.
  /// - Returns: The key.
  static func changesKey(before: String, after: String) -> String {
    "\(before)|\(after)"
  }

  /// The prepared model for this snapshot's inputs, including captures and calendar changes between log readings.
  ///
  /// - Parameters:
  ///   - captures: The listed captures.
  ///   - calendar: The person's calendar.
  ///   - now: The current time.
  /// - Returns: The prepared reading, or nil before the first log reading.
  func prepared(captures: [Screenshot], calendar: Calendar, now: Date) -> ViewerActivityPreparation? {
    guard let log, let readAt else { return nil }
    if let preparation, preparation.matches(log: log, captures: captures, readAt: readAt, calendar: calendar, now: now)
    {
      return preparation
    }
    return ViewerActivityPreparation(
      log: log,
      captures: captures,
      readAt: readAt,
      calendar: calendar,
      now: now,
      preserving: moments
    )
  }
}
