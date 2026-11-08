import Testing

@testable import AtlasUI

@Suite("ActivityArrivals")
@MainActor
struct ActivityArrivalsTests {
  /// Activity listing `sessions` by identity, newest first, in one day.
  ///
  /// - Parameters:
  ///   - sessions: The sessions' identities.
  ///   - chosen: Whose work the list shows.
  ///   - filter: The filter chosen.
  ///   - query: The sidebar's search.
  /// - Returns: The pane.
  static func state(
    _ sessions: [String],
    chosen: ViewerSessionChoice = .all,
    filter: ViewerActivityFilter = .all,
    query: String = ""
  ) -> ViewerActivityState {
    var filters = PreviewStates.activityFilters()
    filters.selected = filter
    return ViewerActivityState(
      session: chosen,
      meta: "",
      filters: filters,
      days: [
        ViewerActivityDay(
          id: "today",
          title: "",
          sessions: sessions.map {
            ViewerOutlineSession(id: $0, icon: nil, agent: "You", summary: $0, time: "10:31", runs: [])
          }
        )
      ],
      query: query
    )
  }

  @Test("scrolled down, new sessions above every one shown wait, and one arriving below comes in")
  func newSessionsAboveWait() {
    var arrivals = ActivityArrivals()
    arrivals.meet(Self.state(["b", "c"]), isAtTop: true)
    let next = Self.state(["new 2", "new 1", "b", "between", "c"])
    #expect(arrivals.waiting(in: next, isAtTop: false) == ["new 2", "new 1"])
    arrivals.meet(next, isAtTop: false)
    let shown = ActivityArrivals.shown(next.days, without: arrivals.waiting(in: next, isAtTop: false))
    #expect(shown.flatMap(\.sessions).map(\.id) == ["b", "between", "c"])
    #expect(ActivityArrivals.words(2) == "2 new sessions")
  }

  @Test("waiting sessions keep waiting as more arrive, until the list lets them in")
  func waitingSessionsWaitUntilReleased() {
    var arrivals = ActivityArrivals()
    arrivals.meet(Self.state(["b"]), isAtTop: true)
    arrivals.meet(Self.state(["a", "b"]), isAtTop: false)
    let more = Self.state(["z", "a", "b"])
    #expect(arrivals.waiting(in: more, isAtTop: false) == ["z", "a"])
    arrivals.meet(more, isAtTop: false)
    #expect(arrivals.waiting(in: more, isAtTop: true) == ["z", "a"], "back at the top, they wait for the release")
    arrivals.release()
    #expect(arrivals.waiting(in: more, isAtTop: true).isEmpty)
    #expect(ActivityArrivals.words(0) == nil)
  }

  @Test("at the top, on the first reading, or as the person picks a filter or searches, nothing waits")
  func nothingWaitsOnThePersonsOwnChange() {
    var arrivals = ActivityArrivals()
    #expect(arrivals.waiting(in: Self.state(["a", "b"]), isAtTop: false).isEmpty, "the first reading")
    arrivals.meet(Self.state(["b"]), isAtTop: true)
    #expect(arrivals.waiting(in: Self.state(["a", "b"]), isAtTop: true).isEmpty, "at the top")
    #expect(arrivals.waiting(in: Self.state(["a", "b"], filter: .problems), isAtTop: false).isEmpty, "a filter")
    arrivals.meet(Self.state(["b"], query: "Chrome"), isAtTop: false)
    #expect(arrivals.waiting(in: Self.state(["a", "b"]), isAtTop: false).isEmpty, "a search cleared")
    #expect(ActivityArrivals.words(1) == "1 new session")
  }

  @Test("a new session choice swaps the list without arrivals, even between sessions of one name")
  func aNewSessionChoiceSwapsTheList() {
    var arrivals = ActivityArrivals()
    arrivals.meet(Self.state(["a"], chosen: .one("first")), isAtTop: false)
    let other = Self.state(["new 2", "new 1", "a"], chosen: .one("second"))
    #expect(other.title == Self.state(["a"], chosen: .one("first")).title, "both sessions go by one name")
    #expect(arrivals.waiting(in: other, isAtTop: false).isEmpty)
    arrivals.meet(other, isAtTop: false)
    let more = Self.state(["newer", "new 2", "new 1", "a"], chosen: .one("second"))
    #expect(arrivals.waiting(in: more, isAtTop: false) == ["newer"], "new work in the new list still waits")
  }

  @Test("a new tab swaps the list without arrivals")
  func aNewTabSwapsTheList() {
    var arrivals = ActivityArrivals()
    arrivals.meet(Self.state(["a"]), isAtTop: false)
    let problems = Self.state(["new", "a"], filter: .problems)
    #expect(arrivals.waiting(in: problems, isAtTop: false).isEmpty)
    #expect(problems.list != Self.state(["a"]).list)
    #expect(problems.list == ViewerActivityListKey(session: .all, filter: .problems))
  }

  @Test("a day left with only waiting sessions is not shown")
  func aDayOfWaitingSessionsIsHidden() {
    let days = [
      ViewerActivityDay(
        id: "tomorrow",
        title: "Today",
        sessions: [ViewerOutlineSession(id: "a", icon: nil, agent: "You", summary: "a", time: "00:01", runs: [])]
      ),
      ViewerActivityDay(
        id: "today",
        title: "Yesterday",
        sessions: [ViewerOutlineSession(id: "b", icon: nil, agent: "You", summary: "b", time: "23:59", runs: [])]
      ),
    ]
    #expect(ActivityArrivals.shown(days, without: ["a"]).map(\.id) == ["today"])
  }
}
