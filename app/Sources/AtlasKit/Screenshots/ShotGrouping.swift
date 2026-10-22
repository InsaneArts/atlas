import Foundation

/// The Screenshots tab's rules, all pure: which stretch of time a capture falls in, how many were taken today, what
/// a search by time keeps, and where Previous and Next go. Time comes in and is never read: the time now and the
/// person's calendar.
package struct ShotGrouping: Sendable {
  /// 3600 s: the board's "Today" is the last hour, the captures of the work under way; the rest of today reads by the
  /// part of the day it was taken in (MacShots puts 10:31 under Today and 10:12 under This morning).
  package static let lastHour: TimeInterval = 3600
  /// 12: the hour the afternoon starts, as people say it.
  static let noon = 12
  /// 18: the hour the evening starts, as people say it.
  static let evening = 18

  /// The time now, which today and the last hour are measured from.
  package var now: Date
  /// The calendar that says which day and hour a time falls on, in the person's time zone.
  package var calendar: Calendar

  /// Creates the rules at a moment.
  ///
  /// - Parameters:
  ///   - now: The time now.
  ///   - calendar: The calendar that says which day and hour a time falls on, in the person's time zone.
  package init(now: Date, calendar: Calendar) {
    self.now = now
    self.calendar = calendar
  }

  /// The stretch of time a capture falls in: the last hour of today (one ahead of the clock included), else the part
  /// of today it was taken in, yesterday or the day it was.
  ///
  /// - Parameter takenAt: When it was taken; nil when the service gives no time.
  /// - Returns: The period.
  package func period(of takenAt: Date?) -> ShotPeriod {
    guard let takenAt else { return .undated }
    if calendar.isDate(takenAt, inSameDayAs: now) {
      if now.timeIntervalSince(takenAt) < Self.lastHour { return .lastHour }
      let hour = calendar.component(.hour, from: takenAt)
      if hour < Self.noon { return .morning }
      return hour < Self.evening ? .afternoon : .evening
    }
    if let yesterday = calendar.date(byAdding: .day, value: -1, to: now),
      calendar.isDate(takenAt, inSameDayAs: yesterday)
    {
      return .yesterday
    }
    return .day(calendar.startOfDay(for: takenAt))
  }

  /// How many captures were taken today, whatever the search: the pane's count.
  ///
  /// - Parameter shots: The screenshots the service lists.
  /// - Returns: The count.
  package func countToday(_ shots: [Screenshot]) -> Int {
    shots
      .filter { shot in shot.takenAt.map { calendar.isDate($0, inSameDayAs: now) } ?? false }
      .count
  }

  /// The groups the tab lists: the captures newest first, those a search by time keeps, each under its stretch of
  /// time.
  ///
  /// - Parameters:
  ///   - shots: The screenshots the service lists.
  ///   - query: The search by time; empty keeps every capture.
  ///   - time: A capture's time of day as the tab shows it, which the search looks in.
  /// - Returns: The groups, newest first, each period once.
  package func days(_ shots: [Screenshot], matching query: String, time: (Date) -> String) -> [ShotDay] {
    let newestFirst = shots.enumerated()
      .sorted { lhs, rhs in
        switch (lhs.element.takenAt, rhs.element.takenAt) {
        case (let left?, let right?) where left != right: left > right
        case (.some, nil): true
        case (nil, .some): false
        default: lhs.offset < rhs.offset
        }
      }
      .map(\.element)
    var days: [ShotDay] = []
    for shot in newestFirst where !shot.name.isEmpty {
      guard Self.matches(time: shot.takenAt.map(time) ?? "", query: query) else { continue }
      let period = period(of: shot.takenAt)
      if let index = days.firstIndex(where: { $0.period == period }) {
        days[index].shots.append(shot)
      } else {
        days.append(ShotDay(period: period, shots: [shot]))
      }
    }
    return days
  }

  /// Whether a capture's time holds a search: "10:31" finds 10:31:44.
  ///
  /// - Parameters:
  ///   - time: The capture's time of day as the tab shows it; empty when it has none.
  ///   - query: The search; empty, or spaces alone, matches every capture.
  /// - Returns: Whether the time holds the search, ignoring case.
  package static func matches(time: String, query: String) -> Bool {
    let words = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !words.isEmpty else { return true }
    return time.localizedCaseInsensitiveContains(words)
  }

  /// Where Previous or Next goes from a capture: the one `offset` places along, never past either end.
  ///
  /// - Parameters:
  ///   - name: The capture shown.
  ///   - offset: −1 for Previous, 1 for Next.
  ///   - names: The captures in the order the tab lists them.
  /// - Returns: The capture to show, or nil at the end or when `name` is not listed.
  package static func step(from name: String, by offset: Int, in names: [String]) -> String? {
    guard let index = names.firstIndex(of: name), names.indices.contains(index + offset) else { return nil }
    return names[index + offset]
  }
}
