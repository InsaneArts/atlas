import AtlasService
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The Activity pane: today's count, the search and the filters, and the lines in their groups, or why none shows
  /// (MacActivity). Times are the person's own clock (``Words/clockTime(_:locale:timeZone:)``), as Details' Logged in
  /// is.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pane.
  static func activity(_ snapshot: ViewerSnapshot) -> ViewerActivityState {
    let facts = snapshot.panes.activity
    let filters = CapsuleTabsState(
      tabs: ViewerActivityFilter.allCases.map { CapsuleTab(id: $0, title: ViewerWords.filter($0)) },
      selected: facts.filter,
      label: ViewerWords.activityFiltersLabel
    )
    guard let log = facts.log, let readAt = facts.readAt else {
      let failed = facts.failure.map {
        EmptyMessageState(headline: ViewerWords.activityFailed, message: Words.sentence($0))
      }
      return ViewerActivityState(meta: "", query: facts.query, filters: filters, groups: [], empty: failed)
    }
    let rules = ActivityGrouping(log: log, readAt: readAt, now: snapshot.date, calendar: snapshot.zonedCalendar)
    let groups = rules.groups(
      filter: activityFilter(facts.filter),
      query: facts.query,
      captures: snapshot.panes.shots.shots ?? []
    )
    return ViewerActivityState(
      meta: ViewerWords.actionsToday(rules.countToday),
      query: facts.query,
      filters: filters,
      groups: groups.map { group($0, snapshot) },
      empty: groups.isEmpty ? activityEmpty(log, facts) : nil
    )
  }

  /// A group as the pane labels it.
  ///
  /// - Parameters:
  ///   - group: The group.
  ///   - snapshot: What the controller holds.
  /// - Returns: The group, its lines made.
  static func group(_ group: ActivityGroup, _ snapshot: ViewerSnapshot) -> ViewerActivityGroup {
    let (id, title): (String, String) =
      switch group.period {
      case .now: ("now", ViewerWords.nowGroup)
      case .earlierToday: ("today", ViewerWords.earlierTodayGroup)
      case .yesterday: ("yesterday", ViewerWords.yesterdayGroup)
      case .day(let start):
        (
          "day-\(Int(start.timeIntervalSince1970))",
          Words.dayName(start, locale: snapshot.locale, timeZone: snapshot.timeZone)
        )
      }
    return ViewerActivityGroup(id: id, title: title, lines: group.runs.map { line($0, snapshot) })
  }

  /// A line: its start on the person's clock, its mark and kind, its words with the repeat count, why it failed, and
  /// the screenshot it took with its thumbnail once read.
  ///
  /// - Parameters:
  ///   - run: The line's rows.
  ///   - snapshot: What the controller holds.
  /// - Returns: The line.
  static func line(_ run: ActivityRun, _ snapshot: ViewerSnapshot) -> ViewerActivityLine {
    let entry = run.entry
    let time =
      run.startedAt.map { Words.clockTime($0, locale: snapshot.locale, timeZone: snapshot.timeZone) } ?? Words.unknown
    let text = ViewerWords.repeated(ActivityGrouping.text(entry), count: run.count)
    let reason = entry.outcome == .failed ? entry.reason : nil
    let lead = lead(entry)
    return ViewerActivityLine(
      id: run.id,
      time: time,
      lead: lead,
      kind: kind(entry),
      text: text,
      reason: reason,
      shot: run.shot.map { ViewerActivityShot(name: $0, thumbnail: snapshot.panes.thumbnails[$0]) },
      label: ViewerWords.lineLabel(time: time, text: text, reason: reason, live: lead == .live)
    )
  }

  /// A line's mark: running before failed, failed before who acted.
  ///
  /// - Parameter entry: The line's row.
  /// - Returns: The mark.
  static func lead(_ entry: ActivityLog.Entry) -> ActivityMarkKind {
    switch (entry.outcome, entry.actor) {
    case (.running, _): .live
    case (.failed, _): .failed
    case (.worked, .you): .person
    case (.worked, .agent), (.worked, .someoneElse): .quiet
    }
  }

  /// What kind of thing a line did, for its tile's symbol (v3 V8): a problem whatever it tried, the person's own as
  /// theirs unless it handed control over, and anyone else's by what it did.
  ///
  /// - Parameter entry: The line's row.
  /// - Returns: The kind.
  static func kind(_ entry: ActivityLog.Entry) -> ViewerActivityKind {
    let action = ActivityAction(entry)
    if entry.outcome == .failed { return .problem }
    if entry.actor == .you, action != .control { return .person }
    switch action {
    case .pointer: return .pointer
    case .keys: return .keys
    case .scroll: return .scroll
    case .screenshot: return .screenshot
    case .window: return .window
    case .control: return .control
    case .command: return .command
    }
  }

  /// The rule a filter picks.
  ///
  /// - Parameter filter: The pane's filter.
  /// - Returns: AtlasService's.
  static func activityFilter(_ filter: ViewerActivityFilter) -> ActivityFilter {
    switch filter {
    case .all: .all
    case .agent: .agent
    case .you: .you
    case .problems: .problems
    }
  }

  /// Why no line shows: nothing done yet, a search that matches nothing, or a filter with nothing in it.
  ///
  /// - Parameters:
  ///   - log: The log read.
  ///   - facts: The pane's search and filter.
  /// - Returns: The message.
  static func activityEmpty(_ log: ActivityLog, _ facts: ViewerActivityFacts) -> EmptyMessageState {
    if log.entries.isEmpty {
      return EmptyMessageState(headline: ViewerWords.noActivity, message: ViewerWords.noActivityMessage)
    }
    if !facts.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      return EmptyMessageState(
        headline: ViewerWords.noActivityMatch(facts.query),
        message: ViewerWords.noActivityMatchMessage
      )
    }
    if facts.filter == .problems {
      return EmptyMessageState(headline: ViewerWords.noProblems, message: ViewerWords.noProblemsMessage)
    }
    return EmptyMessageState(headline: ViewerWords.noneFiltered, message: ViewerWords.noneFilteredMessage)
  }
}
