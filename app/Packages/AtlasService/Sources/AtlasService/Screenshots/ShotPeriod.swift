import Foundation

/// The stretch of time the Screenshots tab groups a capture under, newest first.
public enum ShotPeriod: Hashable, Sendable {
  /// Within the last hour, today: the board's "Today".
  case lastHour
  /// Earlier today, before noon.
  case morning
  /// Earlier today, from noon until six.
  case afternoon
  /// Earlier today, from six on.
  case evening
  /// Yesterday.
  case yesterday
  /// A day before yesterday.
  ///
  /// - Parameter start: The day's first moment.
  case day(Date)
  /// A capture the service gives no time for.
  case undated
}
