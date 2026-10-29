import Foundation

/// The Activity tab's rules, all pure: when each row started, which stretch of time it falls in, what the filters and
/// the search keep, which repeats read as one line, how many actions were done today, what a line says, and which
/// capture a screenshot row took. Time comes in and is never read: when the log was read, the time now and the
/// person's calendar.
public struct ActivityGrouping: Sendable {
  /// 120 s: how recent a row is to read as Now. An agent's run of work is a string of actions seconds apart;
  /// two minutes holds that run whole, while a pause longer than a glance away moves it to Earlier today. The
  /// app's `Words.ago` stops counting in seconds at 90, so Now never outlasts "1 min ago" by much.
  static let nowSpan: TimeInterval = 120
  /// 3 s: how far a capture's time may lie from its screenshot row's start and still be that row's. The service writes
  /// the row a moment after the capture's file, and a row's age comes in whole seconds, so its start reads up to a
  /// second late; three seconds holds both with room. The nearest capture wins and each is taken once, so two
  /// screenshots a second apart still find their own.
  static let shotTolerance: TimeInterval = 3
  /// The operation a screenshot row names.
  public static let screenshotOperation = "screenshot"

  /// The log, newest first.
  var log: ActivityLog
  /// When the log was read: each row's age counts back from here.
  var readAt: Date
  /// The time now, which Now and today are measured from.
  var now: Date
  /// The calendar that says which day a time falls on, in the person's time zone.
  var calendar: Calendar

  /// Creates the rules over one reading of the log.
  ///
  /// - Parameters:
  ///   - log: The log, newest first.
  ///   - readAt: When it was read.
  ///   - now: The time now.
  ///   - calendar: The calendar that says which day a time falls on, in the person's time zone.
  public init(log: ActivityLog, readAt: Date, now: Date, calendar: Calendar) {
    self.log = log
    self.readAt = readAt
    self.now = now
    self.calendar = calendar
  }

  /// When a row started: the start the service gives, the same at every reading, else its age counted back from when
  /// the log was read.
  ///
  /// - Parameter entry: The row.
  /// - Returns: Its start, or nil when the row gives neither.
  func start(of entry: ActivityLog.Entry) -> Date? {
    entry.startedAt ?? entry.ageSeconds.map { readAt.addingTimeInterval(-$0) }
  }

  /// The stretch of time a start falls in: Now within ``nowSpan`` (a start ahead of the clock included), else
  /// earlier today, yesterday or the day it was.
  ///
  /// - Parameter start: When a row started.
  /// - Returns: The period.
  func period(of start: Date) -> ActivityPeriod {
    if now.timeIntervalSince(start) < Self.nowSpan { return .now }
    if calendar.isDate(start, inSameDayAs: now) { return .earlierToday }
    if let yesterday = calendar.date(byAdding: .day, value: -1, to: now),
      calendar.isDate(start, inSameDayAs: yesterday)
    {
      return .yesterday
    }
    return .day(calendar.startOfDay(for: start))
  }

  /// How many rows started today, whatever the filter or the search: the pane's count.
  public var countToday: Int {
    countToday(in: .all)
  }

  /// How many of a scope's rows started today, whatever the filter or the search: the pane's count for one host
  /// session.
  ///
  /// - Parameter scope: Whose rows to count.
  /// - Returns: The count.
  public func countToday(in scope: ActivityScope) -> Int {
    log.entries
      .filter { entry in
        scope.admits(entry)
          && start(of: entry)
            .map { calendar.isDate($0, inSameDayAs: now) } ?? false
      }
      .count
  }

  /// The capture each screenshot row that worked took: the one the row names, when the screenshots list it; for a row
  /// that names none, as an older service's, the nearest one within ``shotTolerance`` of its start, each capture taken
  /// once, newest rows first. A row whose named capture is gone has none, and no other row ever has one.
  ///
  /// - Parameter captures: The screenshots the service lists.
  /// - Returns: Each matched row's capture name, by the row's id.
  func shots(in captures: [Screenshot]) -> [String: String] {
    let listed = Set(
      captures.map(\.name)
        .filter { !$0.isEmpty }
    )
    let taking = log.entries.filter { $0.operation == Self.screenshotOperation && $0.outcome == .worked }
    var found: [String: String] = [:]
    for entry in taking {
      guard let capture = entry.capture, listed.contains(capture) else { continue }
      found[entry.id] = capture
    }
    var taken = Set(found.values)
    for entry in taking where entry.capture == nil {
      guard let started = start(of: entry) else { continue }
      let nearest =
        captures
        .filter { !$0.name.isEmpty && !taken.contains($0.name) }
        .compactMap { shot in shot.takenAt.map { (name: shot.name, gap: abs($0.timeIntervalSince(started))) } }
        .filter { $0.gap <= Self.shotTolerance }
        .min { $0.gap < $1.gap }
      guard let nearest else { continue }
      taken.insert(nearest.name)
      found[entry.id] = nearest.name
    }
    return found
  }

  /// The groups the tab lists: each row under its stretch of time (a row with no time stays with the row before it),
  /// the same action repeated one after another in the log as one line, and the lines the filter and the search
  /// keep. Repeats are found in the log's own order, so actions with another between them never read as one, whatever
  /// the filter or the search hides.
  ///
  /// - Parameters:
  ///   - filter: Which rows to list.
  ///   - query: What to search for in what was done; empty keeps every row.
  ///   - captures: The screenshots the service lists, for the screenshot rows.
  ///   - scope: Whose rows to list; every row by default.
  /// - Returns: The groups, newest first, each period once.
  public func groups(
    filter: ActivityFilter,
    query: String,
    captures: [Screenshot],
    scope: ActivityScope = .all
  ) -> [ActivityGroup] {
    var groups: [ActivityGroup] = []
    for (period, run) in runs(captures: captures)
    where scope.admits(run.entry) && filter.admits(run.entry) && Self.matches(run.entry, query: query) {
      if let index = groups.firstIndex(where: { $0.period == period }) {
        groups[index].runs.append(run)
      } else {
        groups.append(ActivityGroup(period: period, runs: [run]))
      }
    }
    return groups
  }

  /// The lines the pane lists while it shows every host session: a group for each session in `order`, Terminal's
  /// included where `order` names it, then for any other session the log names, newest first, then the person's own
  /// rows as You, and the rows no host named as Terminal last while `order` leaves it out; each with its newest `lines`
  /// lines and its total. The filter and the search apply inside every group, and a group they leave empty is left out.
  /// Repeats are found in the log's own order, as ``groups(filter:query:captures:scope:)`` finds them.
  ///
  /// - Parameters:
  ///   - filter: Which rows to list.
  ///   - query: What to search for in what was done; empty keeps every row.
  ///   - captures: The screenshots the service lists, for the screenshot rows.
  ///   - order: The sessions' keys in the order to list them, the status's; empty for Terminal.
  ///   - lines: The most lines a group shows.
  /// - Returns: The groups, in order.
  public func sessionGroups(
    filter: ActivityFilter,
    query: String,
    captures: [Screenshot],
    order: [String],
    lines: Int
  ) -> [ActivitySessionGroup] {
    var found: [String?: [ActivityRun]] = [:]
    var named: [String] = []
    for (_, run) in runs(captures: captures) where filter.admits(run.entry) && Self.matches(run.entry, query: query) {
      let key = run.entry.groupKey
      if let key, !key.isEmpty, found[key] == nil { named.append(key) }
      found[key, default: []].append(run)
    }
    var keys: [String?] = []
    for key in order + named where !keys.contains(key) { keys.append(key) }
    keys.append(nil)
    if !keys.contains("") { keys.append("") }
    return keys.compactMap { key in
      guard let runs = found[key], !runs.isEmpty else { return nil }
      return ActivitySessionGroup(key: key, runs: Array(runs.prefix(lines)), total: runs.count)
    }
  }

  /// Every line of the log in its own order, each with its stretch of time: a row with no time stays with the row
  /// before it, and a row that repeats the line just above it in the same stretch joins it.
  ///
  /// - Parameter captures: The screenshots the service lists, for the screenshot rows.
  /// - Returns: The lines, newest first.
  func runs(captures: [Screenshot]) -> [(period: ActivityPeriod, run: ActivityRun)] {
    let shots = shots(in: captures)
    var runs: [(period: ActivityPeriod, run: ActivityRun)] = []
    var period = ActivityPeriod.now
    for entry in log.entries {
      let started = start(of: entry)
      if let started { period = self.period(of: started) }
      let run = ActivityRun(entry: entry, startedAt: started, shot: shots[entry.id])
      if let last = runs.last, last.period == period, Self.repeats(last.run, run) {
        runs[runs.count - 1].run = ActivityRun(entry: entry, startedAt: started, count: last.run.count + 1)
      } else {
        runs.append((period, run))
      }
    }
    return runs
  }

  /// Whether a row is the same action as the line just above it, by the same actor in the same host session, with the
  /// same outcome: a running row and a screenshot row always stand alone.
  ///
  /// - Parameters:
  ///   - newer: The line above.
  ///   - older: The row under it.
  /// - Returns: Whether the row joins the line.
  static func repeats(_ newer: ActivityRun, _ older: ActivityRun) -> Bool {
    let above = newer.entry
    let below = older.entry
    guard above.outcome != .running, above.operation != screenshotOperation, below.operation != screenshotOperation
    else { return false }
    return above.actor == below.actor && above.session == below.session && above.says == below.says
      && above.outcome == below.outcome && above.operation == below.operation && above.reason == below.reason
  }

  /// Whether a row matches a search: in what was done, never in who did it.
  ///
  /// - Parameters:
  ///   - entry: The row.
  ///   - query: The search; empty, or spaces alone, matches every row.
  /// - Returns: Whether what the row did holds the search, ignoring case and accents.
  static func matches(_ entry: ActivityLog.Entry, query: String) -> Bool {
    let words = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !words.isEmpty else { return true }
    return done(entry)
      .localizedStandardContains(words)
  }

  /// What a line says: the person's own action after "You", anyone else's as a sentence's start.
  ///
  /// - Parameter entry: The row.
  /// - Returns: "You clicked in Opie", or "Scrolled in Opie".
  public static func text(_ entry: ActivityLog.Entry) -> String {
    let done = done(entry)
    switch entry.actor {
    case .you: return "You \(done)"
    case .agent, .someoneElse:
      let first = done.prefix(1)
      return first.uppercased() + done.dropFirst()
    }
  }

  /// What a row did, in the service's words, or plain words when it gives none.
  ///
  /// - Parameter entry: The row.
  /// - Returns: "clicked in Opie", say.
  static func done(_ entry: ActivityLog.Entry) -> String {
    if let says = entry.says { return says }
    return switch entry.outcome {
    case .failed: "couldn't finish an action"
    case .notSent: "didn't send an action"
    case .running, .worked: "did something in the helper"
    }
  }
}
