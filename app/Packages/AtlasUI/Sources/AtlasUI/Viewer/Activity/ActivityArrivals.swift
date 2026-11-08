/// Which sessions arriving at the top of the Activity list wait while the person is scrolled down, so a row never comes
/// in above what they read and nothing they read ever moves. A waiting session comes in once the person scrolls back to
/// the top or asks for it. Only the list's rows wait: the filters, their counts and the sidebar see every session.
struct ActivityArrivals: Equatable {
  /// Every session the list has met, shown or waiting.
  private var met: Set<String> = []
  /// The sessions waiting, as last met.
  private var held: Set<String> = []
  /// The filter chosen as the list was last met.
  private var filter: ViewerActivityFilter?
  /// What the pane was called as the list was last met: "Activity", or the chosen session's name.
  private var title: String?
  /// The sidebar's search as the list was last met.
  private var query: String?

  /// The sessions that wait in `state`: those waiting already that it still lists, and, while the person is scrolled
  /// down, each session never met that stands above every session shown. A new filter, session choice or search is
  /// the person's own change, so none waits then; nor on the list's first reading.
  ///
  /// - Parameters:
  ///   - state: What the list shows.
  ///   - isAtTop: Whether the list sits at its very top.
  /// - Returns: The sessions' identities.
  func waiting(in state: ViewerActivityState, isAtTop: Bool) -> Set<String> {
    guard !met.isEmpty, state.filters.selected == filter, state.title == title, state.query == query else {
      return []
    }
    var waiting = held
    var above = !isAtTop
    for session in state.days.lazy.flatMap(\.sessions) {
      if met.contains(session.id) {
        above = above && held.contains(session.id)
      } else if above {
        waiting.insert(session.id)
      }
    }
    return waiting.intersection(state.days.flatMap(\.sessions).map(\.id))
  }

  /// Meets the list as it is now, so the sessions waiting in it keep waiting and the rest count as shown.
  ///
  /// - Parameters:
  ///   - state: What the list shows.
  ///   - isAtTop: Whether the list sits at its very top.
  mutating func meet(_ state: ViewerActivityState, isAtTop: Bool) {
    held = waiting(in: state, isAtTop: isAtTop)
    met.formUnion(state.days.flatMap(\.sessions).map(\.id))
    filter = state.filters.selected
    title = state.title
    query = state.query
  }

  /// Lets every waiting session come in.
  mutating func release() {
    held = []
  }

  /// The days as the list shows them: without the waiting sessions, and without a day left with nothing to show.
  ///
  /// - Parameters:
  ///   - days: The days and their sessions.
  ///   - waiting: The sessions waiting.
  /// - Returns: The days shown.
  static func shown(_ days: [ViewerActivityDay], without waiting: Set<String>) -> [ViewerActivityDay] {
    guard !waiting.isEmpty else { return days }
    return days.compactMap { day in
      var day = day
      day.sessions.removeAll { waiting.contains($0.id) }
      return day.sessions.isEmpty && day.more == nil ? nil : day
    }
  }

  /// What the line over the list says of the sessions waiting, such as "1 new session" or "3 new sessions".
  ///
  /// - Parameter count: How many wait.
  /// - Returns: The words, or nil while none waits.
  static func words(_ count: Int) -> String? {
    switch count {
    case 0: nil
    case 1: "1 new session"
    default: "\(count) new sessions"
    }
  }
}
