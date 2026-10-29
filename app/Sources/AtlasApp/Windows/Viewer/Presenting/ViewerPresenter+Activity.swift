import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The Activity pane: its title, today's count, the search and the filters, and the lines in their groups, or why
  /// none shows. While two or more host sessions are live and every session's work shows, the lines are grouped by
  /// session in the status's order, unless the person's own filter leaves only theirs; with one session chosen, its
  /// lines alone, grouped by time under its name, with All and Problems only. With nothing in the log yet the header
  /// says nothing beside its title, and there is nothing to search or filter. Times are the person's own clock
  /// (``Words/clockTime(_:locale:timeZone:)``), as Details' Logged in is.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pane.
  static func activity(_ snapshot: ViewerSnapshot) -> ViewerActivityState {
    let facts = snapshot.panes.activity
    let chosen = snapshot.shownSessions.key
    let resolved = chosen.map { resolvedSession($0, snapshot) }
    let offered = chosen == nil ? ViewerActivityFilter.allCases : [.all, .problems]
    let filter = offered.contains(facts.filter) ? facts.filter : .all
    let filters = CapsuleTabsState(
      tabs: offered.map { CapsuleTab(id: $0, title: ViewerWords.filter($0)) },
      selected: filter,
      label: ViewerWords.activityFiltersLabel
    )
    let title = resolved?.name ?? ViewerWords.tab(.activity)
    let titleIcon = resolved?.icon
    guard let log = facts.log, let readAt = facts.readAt else {
      let failed = facts.failure.map {
        EmptyMessageState(headline: ViewerWords.activityFailed, message: Words.sentence($0))
      }
      return ViewerActivityState(
        title: title,
        titleIcon: titleIcon,
        meta: "",
        query: facts.query,
        filters: filters,
        groups: [],
        empty: failed,
        isSearchable: false
      )
    }
    let rules = ActivityGrouping(log: log, readAt: readAt, now: snapshot.date, calendar: snapshot.zonedCalendar)
    let captures = snapshot.panes.shots.shots ?? []
    let scope = ActivityScope(snapshot.shownSessions)
    var state = ViewerActivityState(
      title: title,
      titleIcon: titleIcon,
      meta: log.entries.isEmpty ? "" : ViewerWords.actionsToday(rules.countToday(in: scope)),
      query: facts.query,
      filters: filters,
      groups: [],
      isSearchable: !log.entries.isEmpty,
      findRequests: facts.findRequests
    )
    if chosen == nil, snapshot.reading.statusReading.showsSessions, filter != .you {
      let groups = rules.sessionGroups(
        filter: activityFilter(filter),
        query: facts.query,
        captures: captures,
        order: sessionOrder(snapshot),
        lines: ViewerActivityState.mostSessionLines
      )
      state.sessions = groups.map { sessionGroup($0, snapshot) }
      state.empty = groups.isEmpty ? activityEmpty(log, facts) : nil
      return state
    }
    let groups = rules.groups(filter: activityFilter(filter), query: facts.query, captures: captures, scope: scope)
    state.groups = groups.map { group($0, snapshot) }
    if groups.isEmpty {
      let nothingFromIt =
        chosen != nil
        && rules.groups(filter: .all, query: "", captures: [], scope: scope)
          .isEmpty
      state.empty =
        nothingFromIt
        ? EmptyMessageState(headline: ViewerWords.noActivity, message: ViewerWords.noSessionActivityMessage)
        : activityEmpty(log, facts)
    }
    return state
  }

  /// The order Activity lists the host sessions in while it shows every session's work: the live ones in the
  /// status's order, which is by state, Terminal's included, so the one needing the person comes first; then the live
  /// ones the status had no room for, from the sessions' list; then those the list says ended, newest first.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The sessions' keys; empty for Terminal.
  static func sessionOrder(_ snapshot: ViewerSnapshot) -> [String] {
    let list = snapshot.panes.sessions.list
    let listed = snapshot.reading.statusReading.liveSessions.map(\.key)
    let rest = (list?.live ?? [])
      .map(\.key)
    let ended = (list?.sessions ?? [])
      .filter(\.isEnded)
      .map(\.key)
    var order: [String] = []
    for key in listed + rest + ended where !order.contains(key) { order.append(key) }
    return order
  }

  /// One host session's group as the pane heads it: its icon, its name, its state in its tone
  /// (``ViewerResolvedSession/state``), none for a session nothing names, and Show all, naming the session to
  /// VoiceOver, while it holds more lines than it lists. The person's own lines are "You", with the person's blue dot,
  /// and their Show all picks the person's own filter.
  ///
  /// - Parameters:
  ///   - group: The group.
  ///   - snapshot: What the controller holds.
  /// - Returns: The group, its lines made.
  static func sessionGroup(_ group: ActivitySessionGroup, _ snapshot: ViewerSnapshot) -> ViewerActivitySessionGroup {
    let lines = group.runs.map { line($0, snapshot) }
    let hasMore = group.total > group.runs.count
    guard let key = group.key else {
      return ViewerActivitySessionGroup(
        id: "you",
        icon: nil,
        name: ViewerWords.youGroup,
        lines: lines,
        showAll: hasMore ? showAll(group, name: ViewerWords.youGroup, event: .filterActivity(.you)) : nil
      )
    }
    let session = resolvedSession(key, snapshot)
    let name = session.name
    return ViewerActivitySessionGroup(
      id: "session-\(key)",
      icon: session.icon,
      name: name,
      state: session.state.map(Words.sessionState) ?? "",
      tone: session.state?.tone ?? .quiet,
      lines: lines,
      showAll: hasMore ? showAll(group, name: name, event: .chooseSession(.one(key))) : nil
    )
  }

  /// Show all under a group that holds more lines than it lists, naming the group to VoiceOver.
  ///
  /// - Parameters:
  ///   - group: The group.
  ///   - name: The group's name.
  ///   - event: What it sends.
  /// - Returns: Show all.
  static func showAll(_ group: ActivitySessionGroup, name: String, event: ViewerEvent) -> ViewerShowAll {
    ViewerShowAll(
      title: ViewerWords.showAll(group.total),
      label: ViewerWords.showAll(group.total, from: name),
      event: event
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

  /// A line: its start on the person's clock, its mark and kind, its words with the repeat count, why it failed or was
  /// not sent, and the screenshot it took with its thumbnail once read. The action whose outcome is unknown, while
  /// sessions show and the helper needs the person, is marked as a problem, says that every session waits, and offers
  /// Show Screen and Recover….
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
    let offersRecovery = run.id == snapshot.reading.statusReading.causer?.uncertainRow
    let reason: String? =
      switch (offersRecovery, entry.outcome) {
      case (true, _): ViewerWords.unfinishedReason
      case (false, .failed), (false, .notSent): entry.reason
      case (false, .running), (false, .worked): nil
      }
    let mark: ActivityMarkKind = offersRecovery ? .failed : lead(entry)
    return ViewerActivityLine(
      id: run.id,
      time: time,
      lead: mark,
      kind: offersRecovery ? .problem : kind(entry),
      text: text,
      reason: reason,
      shot: run.shot.map { ViewerActivityShot(name: $0, thumbnail: snapshot.panes.thumbnails[$0]) },
      offersRecovery: offersRecovery,
      label: ViewerWords.lineLabel(time: time, text: text, reason: reason, live: mark == .live)
    )
  }

  /// A line's mark: running before failed or not sent, those before who acted.
  ///
  /// - Parameter entry: The line's row.
  /// - Returns: The mark.
  static func lead(_ entry: ActivityLog.Entry) -> ActivityMarkKind {
    switch (entry.outcome, entry.actor) {
    case (.running, _): .live
    case (.failed, _): .failed
    case (.notSent, _): .notSent
    case (.worked, .you): .person
    case (.worked, .agent), (.worked, .someoneElse): .quiet
    }
  }

  /// What kind of thing a line did, for its tile's symbol: a problem whatever it tried, the person's own as
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
    case .typing, .keys: return .keys
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
