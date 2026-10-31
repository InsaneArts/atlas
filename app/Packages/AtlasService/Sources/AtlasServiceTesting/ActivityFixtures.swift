import AtlasService
import Foundation

/// Activity rows and a fixed reading shared by service and app tests.
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

  /// Builds a row, leaving optional evidence absent unless supplied.
  ///
  /// - Parameters:
  ///   - id: The row's identity.
  ///   - logicalId: The action's identity; the row's identity by default.
  ///   - says: What it did, in the service's words.
  ///   - outcome: How it went.
  ///   - actor: Who did it.
  ///   - ageSeconds: Its age at the reading, in seconds.
  ///   - startedAt: When it started.
  ///   - reason: Why it failed or what held it.
  ///   - operation: The operation's name.
  ///   - count: How many input actions it carried.
  ///   - session: Its host session's key.
  ///   - capture: Its screenshot's file name.
  ///   - verb: The recorded verb.
  ///   - app: The app's name.
  ///   - label: The recorded target or shortcut.
  ///   - bundleId: The app's bundle identifier.
  ///   - keys: The shortcut's chords as keycaps.
  /// - Returns: The row.
  public static func row(
    id: String,
    logicalId: String? = nil,
    says: String? = nil,
    outcome: ActivityLog.Outcome = .worked,
    actor: ActivityLog.Actor = .agent,
    ageSeconds: Double? = nil,
    startedAt: Date? = nil,
    reason: String? = nil,
    operation: String = "input",
    count: Int? = nil,
    session: String? = nil,
    capture: String? = nil,
    verb: String? = nil,
    app: String? = nil,
    label: String? = nil,
    bundleId: String? = nil,
    keys: [[String]] = []
  ) -> ActivityLog.Entry {
    ActivityLog.Entry(
      id: id,
      logicalId: logicalId,
      says: says,
      outcome: outcome,
      actor: actor,
      ageSeconds: ageSeconds,
      startedAt: startedAt,
      reason: reason,
      operation: operation,
      count: count,
      session: session,
      capture: capture,
      verb: verb,
      app: app,
      label: label,
      bundleId: bundleId,
      keys: keys
    )
  }
}
