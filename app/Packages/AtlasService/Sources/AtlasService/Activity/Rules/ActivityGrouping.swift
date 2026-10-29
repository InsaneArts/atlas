import Foundation

/// Shared Activity rules, all pure: when each row started, what the search keeps, which repeats read as one line,
/// how many actions were done today, what a line says, and which capture a screenshot row took. Time comes in and is never read: when the log was read, the time now and the
/// person's calendar.
public struct ActivityGrouping: Sendable {
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
  /// The time now, which today is measured from.
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

  /// Every line of the log in its own order, each with the part of time it falls in: a row with no time stays with
  /// the row before it, and a row that repeats the line just above it in the same part joins it.
  ///
  /// - Parameters:
  ///   - captures: The screenshots the service lists, for the screenshot rows.
  ///   - first: The part a row with no time falls in before any row has one.
  ///   - part: The part a start falls in, such as its day.
  /// - Returns: The lines, newest first.
  func runs<Part: Equatable>(
    captures: [Screenshot],
    first: Part,
    part: (Date) -> Part
  ) -> [(part: Part, run: ActivityRun)] {
    let shots = shots(in: captures)
    var runs: [(part: Part, run: ActivityRun)] = []
    var current = first
    for entry in log.entries {
      let started = start(of: entry)
      if let started { current = part(started) }
      let run = ActivityRun(entry: entry, startedAt: started, shot: shots[entry.id])
      if let last = runs.last, last.part == current, Self.repeats(last.run, run) {
        runs[runs.count - 1].run = ActivityRun(
          entry: entry,
          startedAt: started,
          count: last.run.count + 1,
          endedAt: last.run.endedAt
        )
      } else {
        runs.append((current, run))
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
