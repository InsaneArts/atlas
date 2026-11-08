import AtlasService
import Foundation

/// A fixed reading shared by service and app tests.
public enum ActivityFixtures {
  /// 10:31:42 on 22 October 2026, in UTC.
  public static let readAt = Date(timeIntervalSince1970: 1_792_665_102)

  /// A Gregorian calendar in UTC, so dates agree on every Mac.
  public static let calendar: Calendar = {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
    return calendar
  }()

  /// Groups rows at the fixed reading time.
  ///
  /// - Parameter entries: The log's rows, newest first.
  /// - Returns: The grouping rules.
  public static func rules(_ entries: [ActivityLog.Entry]) -> ActivityGrouping {
    ActivityGrouping(log: ActivityLog(entries: entries), readAt: readAt, now: readAt, calendar: calendar)
  }
}
