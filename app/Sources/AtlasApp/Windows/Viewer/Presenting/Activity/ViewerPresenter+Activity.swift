import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The Activity pane: whose work it lists and today's count for VoiceOver, the filters with their problem and
  /// screenshot counts, and the moments with their steps, narrowed by the filter and the sidebar's search, or why
  /// nothing shows. Moments form from every row in scope before the filter, so each keeps its identity whichever
  /// filter shows it. With nothing in the log yet there is nothing to filter. Times are the person's own clock.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pane.
  static func activity(_ snapshot: ViewerSnapshot) -> ViewerActivityState {
    let facts = snapshot.panes.activity
    let chosen = snapshot.shownSessions.key
    let title = chosen.map { resolvedSession($0, snapshot).name } ?? ViewerWords.place(.activity)
    guard facts.failure == nil, let log = facts.log, let readAt = facts.readAt,
      let prepared = facts.prepared(
        captures: snapshot.panes.shots.shots ?? [],
        calendar: snapshot.zonedCalendar,
        now: snapshot.date
      )
    else {
      let failed = facts.failure.map {
        EmptyMessageState(headline: ViewerWords.activityFailed, message: Words.sentence($0))
      }
      return ViewerActivityState(
        title: title,
        meta: "",
        filters: activityFilters(facts.filter, counts: [:]),
        days: [],
        empty: failed,
        showsFilters: false
      )
    }
    let rules = ActivityGrouping(log: log, readAt: readAt, now: snapshot.date, calendar: snapshot.zonedCalendar)
    let scope = ActivityScope(snapshot.shownSessions)
    var moments = prepared.moments
    if case .session(let key) = scope {
      moments.moments = moments.moments.filter { $0.stream == .session(key) }
    }
    let query = snapshot.session.search.query
    let shown = moments.shown(filter: activityFilter(facts.filter), query: query)
    let counts = [ViewerActivityFilter.problems, .screenshots].reduce(into: [ViewerActivityFilter: Int]()) {
      $0[$1] = moments.shown(filter: activityFilter($1), query: "")
        .reduce(0) { $0 + $1.runs.count }
    }
    let days = activityDays(shown, moments: moments, snapshot)
    return ViewerActivityState(
      title: title,
      meta: log.entries.isEmpty ? "" : ViewerWords.actionsToday(rules.countToday(in: scope)),
      filters: activityFilters(facts.filter, counts: counts),
      days: days,
      empty: days.isEmpty ? activityEmpty(log, moments, facts, query: query, chosen: chosen != nil) : nil,
      showsFilters: !moments.moments.isEmpty
    )
  }

  /// The filters, the chosen one selected, Problems and Screenshots with how many they hold once they hold any.
  ///
  /// - Parameters:
  ///   - chosen: The filter chosen.
  ///   - counts: How many steps each filter holds, by the filter.
  /// - Returns: The filters.
  static func activityFilters(
    _ chosen: ViewerActivityFilter,
    counts: [ViewerActivityFilter: Int]
  ) -> CapsuleTabsState<ViewerActivityFilter> {
    CapsuleTabsState(
      tabs: ViewerActivityFilter.allCases.map { filter in
        let count = counts[filter] ?? 0
        return CapsuleTab(id: filter, title: ViewerWords.filter(filter), count: count > 0 ? count : nil)
      },
      selected: chosen,
      label: ViewerWords.activityFiltersLabel
    )
  }

  /// The rule a filter picks.
  ///
  /// - Parameter filter: The pane's filter.
  /// - Returns: AtlasService's.
  static func activityFilter(_ filter: ViewerActivityFilter) -> ActivityFilter {
    switch filter {
    case .all: .all
    case .problems: .problems
    case .screenshots: .screenshots
    case .typing: .typing
    }
  }

  /// Why nothing shows: nothing done yet, nothing from the chosen session, a search that matches nothing, or a filter
  /// with nothing in it.
  ///
  /// - Parameters:
  ///   - log: The log read.
  ///   - moments: Every moment in scope.
  ///   - facts: The pane's filter.
  ///   - query: The sidebar's search.
  ///   - chosen: Whether one session is chosen.
  /// - Returns: The message.
  static func activityEmpty(
    _ log: ActivityLog,
    _ moments: ActivityMoments,
    _ facts: ViewerActivityFacts,
    query: String,
    chosen: Bool
  ) -> EmptyMessageState {
    if moments.moments.isEmpty, chosen {
      return EmptyMessageState(headline: ViewerWords.noActivity, message: ViewerWords.noSessionActivityMessage)
    }
    if log.entries.isEmpty {
      return EmptyMessageState(headline: ViewerWords.noActivity, message: ViewerWords.noActivityMessage)
    }
    if !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      return EmptyMessageState(
        headline: ViewerWords.noActivityMatch(query),
        message: ViewerWords.noActivityMatchMessage
      )
    }
    if facts.filter == .problems {
      return EmptyMessageState(headline: ViewerWords.noProblems, message: ViewerWords.noProblemsMessage)
    }
    return EmptyMessageState(headline: ViewerWords.noneFiltered, message: ViewerWords.noneFilteredMessage)
  }
}
