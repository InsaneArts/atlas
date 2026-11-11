import AtlasService
import AtlasStatus
import Foundation

extension ViewerPresenter {
  /// What the sidebar, Activity and the header call a session: its project folder's name as written, else its agent's
  /// name, such as "atlas" or "Claude Code".
  ///
  /// - Parameters:
  ///   - agent: Its agent's name.
  ///   - project: Its project folder's name; empty for none.
  /// - Returns: The title.
  static func sessionTitle(agent: String, project: String) -> String {
    project.isEmpty ? agent : project
  }

  /// Whether a session never acted in the helper, so the sidebar gives it no row: its record counts no action and the
  /// log holds none of its rows. A session with no record yet is not known to have done nothing.
  ///
  /// - Parameters:
  ///   - key: The session's key.
  ///   - snapshot: What the controller holds.
  /// - Returns: Whether it never acted.
  static func neverActed(_ key: String, _ snapshot: ViewerSnapshot) -> Bool {
    guard let record = snapshot.panes.sessions.record(key) else { return false }
    return record.actions == 0 && sessionRows(key, snapshot).isEmpty
  }

  /// A quiet session's line in the sidebar: its agent and when it started, such as "Claude Code · Wed 22:20", or
  /// "Claude Code · 12:46" for today; the time alone when its title is its agent already.
  ///
  /// - Parameters:
  ///   - agent: Its agent's name.
  ///   - project: Its project folder's name; empty for none.
  ///   - started: When it started.
  ///   - snapshot: What the controller holds.
  /// - Returns: The line.
  static func quietLine(agent: String, project: String, started: Date?, _ snapshot: ViewerSnapshot) -> String {
    let when = started.map { date in
      let calendar = snapshot.zonedCalendar
      let style = Date.FormatStyle(locale: snapshot.locale, calendar: calendar, timeZone: snapshot.timeZone)
      let time = Words.clockMinute(date, locale: snapshot.locale, calendar: calendar, timeZone: snapshot.timeZone)
      guard !calendar.isDate(date, inSameDayAs: snapshot.date) else { return time }
      return "\(date.formatted(style.weekday(.abbreviated))) \(time)"
    }
    return ViewerWords.dotted([project.isEmpty ? "" : agent, when ?? ""])
  }

  /// An ended session's line in the sidebar, which its one-line row draws at its trailing edge: when it started, such
  /// as "12:46" today, "Wed" within the last six days, else "Nov 8"; empty while unknown.
  ///
  /// - Parameters:
  ///   - started: When it started.
  ///   - snapshot: What the controller holds.
  /// - Returns: The line.
  static func endedLine(started: Date?, _ snapshot: ViewerSnapshot) -> String {
    guard let started else { return "" }
    return Words.shortDay(
      started,
      now: snapshot.date,
      locale: snapshot.locale,
      calendar: snapshot.zonedCalendar,
      timeZone: snapshot.timeZone
    )
  }

  /// What an ended session's row leaves to its tooltip and VoiceOver: its agent and the day and time it started, such
  /// as "Claude Code · Wed 11 Nov 22:20"; the day and time alone when its title is its agent already.
  ///
  /// - Parameters:
  ///   - agent: Its agent's name.
  ///   - project: Its project folder's name; empty for none.
  ///   - started: When it started.
  ///   - snapshot: What the controller holds.
  /// - Returns: The line.
  static func startedLine(agent: String, project: String, started: Date?, _ snapshot: ViewerSnapshot) -> String {
    let when = started.map { date in
      let calendar = snapshot.zonedCalendar
      let day = Words.dayName(date, locale: snapshot.locale, calendar: calendar, timeZone: snapshot.timeZone)
      let time = Words.clockMinute(date, locale: snapshot.locale, calendar: calendar, timeZone: snapshot.timeZone)
      return "\(day) \(time)"
    }
    return ViewerWords.dotted([project.isEmpty ? "" : agent, when ?? ""])
  }

  /// A session's rows in the log as last read that sent something: a row refused before anything was sent is no work.
  /// None while Activity has not been read.
  ///
  /// - Parameters:
  ///   - key: The session's key.
  ///   - snapshot: What the controller holds.
  /// - Returns: Its rows, newest first.
  static func sessionRows(_ key: String, _ snapshot: ViewerSnapshot) -> [ActivityLog.Entry] {
    (snapshot.panes.activity.log?.entries ?? [])
      .filter { $0.groupKey == key && $0.outcome != .notSent }
  }
}
