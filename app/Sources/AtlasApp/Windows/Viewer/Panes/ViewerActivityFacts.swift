import AtlasService
import AtlasUI
import Foundation

/// What the Activity pane last read, and what the person asked of it: the filter, the sessions and runs they opened
/// or folded, the runs whose pictures they opened in place and the days they asked to list every session of, each kept
/// for the window's life, and the picture in the sheet or the run replayed with the pictures read for them. The
/// sidebar's search narrows it.
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
  /// The person's own choice for a run, by its id: true to open it, false to fold it. A choice, not a flip of the
  /// default, so a run the person opened stays open as newer actions join it.
  var choices: [String: Bool] = [:]
  /// The person's own choice for a session, by its id: true to open it, false to fold it.
  var sessionChoices: [String: Bool] = [:]
  /// The runs whose pictures the person opened in place, by their ids: true once they asked for every one.
  var pictureRuns: [String: Bool] = [:]
  /// The days the person asked to list every session of, by their ids.
  var shownDays: Set<String> = []
  /// The picture in the sheet over the window; nil shows none.
  var sheet: ViewerPictureFocus?
  /// The task replayed over the window; nil shows none.
  var replay: ViewerReplayFocus?
  /// The pictures read as taken, for the sheet and the replay, by file name.
  var pictures: [String: DesktopPicture] = [:]

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
