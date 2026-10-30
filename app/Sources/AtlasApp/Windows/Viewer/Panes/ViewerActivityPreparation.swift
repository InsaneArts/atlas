import AtlasService
import Foundation

/// One value-owned preparation. Reconciliation receives history explicitly, never from another projection.
struct ViewerActivityPreparation {
  /// The reading whose moments were formed.
  let log: ActivityLog
  /// The captures matched to its screenshot rows.
  let captures: [Screenshot]
  /// When the reading arrived, anchoring relative ages.
  let readAt: Date
  /// The calendar used to partition days.
  let calendar: Calendar
  /// Today's start, also the initial day for rows without a time.
  let today: Date
  /// The prepared moments, with every stream retained.
  let moments: ActivityMoments

  /// Prepares one reading against the model's preceding ingestion or a snapshot's own values.
  ///
  /// - Parameters:
  ///   - log: The reading.
  ///   - captures: The listed captures.
  ///   - readAt: When the reading arrived.
  ///   - calendar: The person's calendar.
  ///   - now: The current time.
  ///   - previous: The preceding reading's moments, whose overlapping identities survive.
  init(
    log: ActivityLog,
    captures: [Screenshot],
    readAt: Date,
    calendar: Calendar,
    now: Date,
    preserving previous: [ActivityMoment]
  ) {
    self.log = log
    self.captures = captures
    self.readAt = readAt
    self.calendar = calendar
    today = calendar.startOfDay(for: now)
    moments = ActivityMoments(
      ActivityGrouping(log: log, readAt: readAt, now: now, calendar: calendar),
      captures: captures,
      scope: .all,
      preserving: previous
    )
  }

  /// Whether a projection can read this preparation without reforming its moments.
  ///
  /// - Parameters:
  ///   - log: The snapshot's reading.
  ///   - captures: The snapshot's captures.
  ///   - readAt: When the reading arrived.
  ///   - calendar: The snapshot's calendar.
  ///   - now: The snapshot's current time.
  /// - Returns: Whether formation has the same inputs.
  func matches(log: ActivityLog, captures: [Screenshot], readAt: Date, calendar: Calendar, now: Date) -> Bool {
    self.log == log && self.captures == captures && self.readAt == readAt && self.calendar == calendar
      && today == calendar.startOfDay(for: now)
  }
}
