import Foundation

/// The stretch of time the Activity tab groups a row under, newest first.
package enum ActivityPeriod: Hashable, Sendable {
  /// Within the last couple of minutes (``ActivityGrouping/nowSpan``).
  case now
  /// Earlier on today.
  case earlierToday
  /// Yesterday.
  case yesterday
  /// A day before yesterday.
  ///
  /// - Parameter start: The day's first moment.
  case day(Date)
}
