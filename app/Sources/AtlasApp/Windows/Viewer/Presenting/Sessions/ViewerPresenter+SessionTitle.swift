import AtlasService
import AtlasStatus
import Foundation

extension ViewerPresenter {
  /// What the sidebar and the header call a session: what it mostly did and where, from its record's counts, which
  /// outlast the log, else from its rows, such as "Typing in TextEdit" while it works and "Typed in TextEdit" after;
  /// its name when it did nothing Atlas can name.
  ///
  /// - Parameters:
  ///   - key: The session's key; empty for Terminal.
  ///   - working: Whether it works now.
  ///   - fallback: Its name.
  ///   - snapshot: What the controller holds.
  /// - Returns: The title.
  static func sessionTitle(_ key: String, working: Bool, fallback: String, _ snapshot: ViewerSnapshot) -> String {
    let gist = snapshot.panes.sessions.record(key)?.gist ?? ActivityGist(sessionRows(key, snapshot))
    return gist.map { ViewerWords.sessionTitle($0, working: working) } ?? fallback
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

  /// A quiet session's line in the sidebar: its project, or its host with none, and when it started, such as
  /// "atlas-t3-r3 · Wed 22:20", or the time alone for today.
  ///
  /// - Parameters:
  ///   - folder: Its project folder's name.
  ///   - hostLabel: Its host's name.
  ///   - started: When it started.
  ///   - snapshot: What the controller holds.
  /// - Returns: The line.
  static func quietLine(folder: String?, hostLabel: String, started: Date?, _ snapshot: ViewerSnapshot) -> String {
    let place = folder.flatMap { $0.isEmpty ? nil : $0 } ?? hostLabel
    let when = started.map { date in
      let calendar = snapshot.zonedCalendar
      let style = Date.FormatStyle(locale: snapshot.locale, calendar: calendar, timeZone: snapshot.timeZone)
      let time = Words.clockMinute(date, locale: snapshot.locale, calendar: calendar, timeZone: snapshot.timeZone)
      guard !calendar.isDate(date, inSameDayAs: snapshot.date) else { return time }
      return "\(date.formatted(style.weekday(.abbreviated))) \(time)"
    }
    return ViewerWords.dotted([place, when ?? ""])
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
