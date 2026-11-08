import AtlasService
import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The Activity pane: whose work it lists and today's count for VoiceOver, the filters with their problem and
  /// picture counts, and the outline of sessions, runs and steps, narrowed by the filter and the sidebar's search, or
  /// why nothing shows. Runs form from every row in scope before the filter, so each keeps its identity whichever
  /// filter shows it. With nothing in the log yet there is nothing to filter. Times are the person's own clock. The
  /// picture sheet, or a run replayed, covers the window.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The pane.
  static func activity(_ snapshot: ViewerSnapshot) -> ViewerActivityState {
    let facts = snapshot.panes.activity
    let chosen = snapshot.shownSessions.key
    let title = chosen.map { resolvedSession($0, snapshot).name } ?? ViewerWords.place(.activity)
    guard facts.failure == nil, let log = facts.log, let readAt = facts.readAt, let moments = activityMoments(snapshot)
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
    let query = snapshot.session.search.query
    let shown = moments.shown(filter: activityFilter(facts.filter), query: query)
    let counts = [ViewerActivityFilter.problems, .pictures].reduce(into: [ViewerActivityFilter: Int]()) {
      $0[$1] = moments.shown(filter: activityFilter($1), query: "")
        .reduce(0) { $0 + $1.runs.count }
    }
    let days = returningFocus(activityDays(shown, moments: moments, snapshot), snapshot)
    return ViewerActivityState(
      title: title,
      meta: log.entries.isEmpty ? "" : ViewerWords.actionsToday(rules.countToday(in: scope)),
      filters: activityFilters(facts.filter, counts: counts),
      days: days,
      empty: days.isEmpty ? activityEmpty(log, moments, facts, query: query, chosen: chosen != nil) : nil,
      query: query,
      showsFilters: !moments.moments.isEmpty,
      sheet: pictureSheet(moments, snapshot),
      replay: replay(moments, snapshot)
    )
  }

  /// Every run of the reading in scope: the chosen session's, or every session's.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The runs, or nil before the log is read.
  static func activityMoments(_ snapshot: ViewerSnapshot) -> ActivityMoments? {
    let facts = snapshot.panes.activity
    guard
      var moments = facts.prepared(
        captures: snapshot.panes.shots.shots ?? [],
        calendar: snapshot.zonedCalendar,
        now: snapshot.date
      )?
      .moments
    else { return nil }
    if case .session(let key) = ActivityScope(snapshot.shownSessions) {
      moments.moments = moments.moments.filter { $0.stream == .session(key) }
    }
    return moments
  }

  /// The filters, the chosen one selected, Problems and Pictures with how many they hold once they hold any.
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
    case .pictures: .pictures
    case .typing: .typing
    }
  }

  /// Why nothing shows: nothing done yet, nothing from the chosen session, a search that matches nothing, or a filter
  /// with nothing in it.
  ///
  /// - Parameters:
  ///   - log: The log read.
  ///   - moments: Every run in scope.
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
