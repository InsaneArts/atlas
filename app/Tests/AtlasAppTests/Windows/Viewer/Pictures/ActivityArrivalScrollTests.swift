import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Activity arrival scrolling", .serialized, AppKitState())
@MainActor
struct ActivityArrivalScrollTests {
  @Test("a session arriving while the Activity list is scrolled down waits under a line, and the row read never moves")
  func aScrolledArrivalWaits() async throws {
    let (host, scroll, retained, before) = try await Self.scrolledWithArrival()
    #expect(scroll.contentView.bounds.minY == Self.scrolled, "the list did not move")
    #expect(host.frames[retained.key] == retained.value, "the same words stay at the same screen position")
    #expect(try host.pixels(in: retained.value) == before)
    #expect(host.frames["session new session"] == nil, "the new session waits above")
    let line = try #require(host.frames["waiting line"])
    #expect(line.height == ViewerLayout.waitingHeight)
  }

  @Test("scrolling back to the top brings the waiting session in at the top")
  func theWaitingSessionComesIn() async throws {
    let (host, scroll, _, _) = try await Self.scrolledWithArrival()
    scroll.contentView.scroll(to: .zero)
    scroll.reflectScrolledClipView(scroll.contentView)
    try host.render(until: "the new session came in at the top") {
      scroll.contentView.bounds.minY == .zero && host.frames["session new session"] != nil
    }
    let arrived = try #require(host.frames["session new session"])
    let first = try #require(host.frames["session old-0"])
    #expect(arrived.minY < first.minY, "it comes in above the rest")
  }

  @Test("a session arriving while the Activity list sits at its top comes in at once, with no line")
  func anArrivalAtTheTopComesInAtOnce() async throws {
    var activity = Self.sessions(40)
    let host = ViewerHost(Self.state(activity), animatesArrivals: false)
    await Self.render(host)
    activity.days[0].sessions.insert(Self.arrival, at: 0)
    host.show(Self.state(activity))
    await Self.render(host)
    let scroll = try #require(host.scrollViews.first)
    #expect(scroll.contentView.bounds.minY == .zero)
    #expect(host.frames["session new session"] != nil)
    #expect(host.frames["waiting line"] == nil)
  }

  @Test("a run arriving at the top of an open session keeps a scrolled-down reader's line in place")
  func runArrivalsLeaveScrolledRunsInPlace() async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    var state = PreviewStates.viewerActivity
    let runs =
      (0..<40)
      .map {
        ViewerOutlineRun(id: "old-\($0)", icon: .atlas, name: "Commands", summary: "Ran command \($0)", time: "10:31")
      }
    let session = ViewerOutlineSession(
      id: "long session",
      icon: nil,
      agent: "You",
      summary: "Ran commands",
      time: "10:31",
      runs: runs
    )
    var activity = ViewerActivityState(
      meta: "",
      filters: PreviewStates.activityFilters(),
      days: [ViewerActivityDay(id: "today", title: "", sessions: [session])]
    )
    state.pane = .activity(activity)
    let host = ViewerHost(state, animatesArrivals: false)
    await Self.render(host)
    let scroll = try #require(host.scrollViews.first)
    let unscrolledFrames = host.frames
    scroll.contentView.scroll(to: CGPoint(x: 0, y: 450))
    scroll.reflectScrolledClipView(scroll.contentView)
    await Self.render(host)
    let viewport = host.frameFromTop(of: scroll)
    // Lazy rows that left the screen retain their old reports. Read only a row measured after scrolling.
    let retained = try #require(
      host.frames.first { key, frame in
        key.hasPrefix("run old-") && unscrolledFrames[key] != frame
          && frame.minY > viewport.minY + 40 && frame.maxY < viewport.maxY - 40
      }
    )
    let before = try host.pixels(in: retained.value)
    let arrival = ViewerOutlineRun(id: "new run", icon: .atlas, name: "Commands", summary: "Arrived now", time: "10:31")
    activity.days[0].sessions[0].runs.insert(arrival, at: 0)
    state.pane = .activity(activity)
    Motion.Pose.hold(at: .zero)
    host.show(state)
    await Self.render(host)
    #expect(scroll.contentView.bounds.minY > 0)
    #expect(host.frames[retained.key] == retained.value, "the same run stays at the same screen position")
    #expect(try host.pixels(in: retained.value) == before)
  }

  /// 450: how far the list is scrolled down before a session arrives.
  static let scrolled: CGFloat = 450

  /// The session that arrives.
  static let arrival = ViewerOutlineSession(
    id: "new session",
    icon: nil,
    agent: "You",
    summary: "Arrived now",
    time: "10:31",
    runs: [ViewerOutlineRun(id: "new run", icon: .atlas, name: "Commands", summary: "Arrived now", time: "10:31")]
  )

  /// Activity with `count` sessions of one run each, newest first.
  ///
  /// - Parameter count: How many sessions.
  /// - Returns: The pane.
  static func sessions(_ count: Int) -> ViewerActivityState {
    let sessions =
      (0..<count)
      .map {
        ViewerOutlineSession(
          id: "old-\($0)",
          icon: nil,
          agent: "You",
          summary: "Session \($0)",
          time: "10:31",
          runs: [
            ViewerOutlineRun(id: "run-\($0)", icon: .atlas, name: "Commands", summary: "Ran a command", time: "10:31")
          ]
        )
      }
    return ViewerActivityState(
      meta: "",
      filters: PreviewStates.activityFilters(),
      days: [ViewerActivityDay(id: "today", title: "", sessions: sessions)]
    )
  }

  /// The window showing `activity`.
  ///
  /// - Parameter activity: The pane.
  /// - Returns: What the window shows.
  static func state(_ activity: ViewerActivityState) -> ViewerState {
    var state = PreviewStates.viewerActivity
    state.pane = .activity(activity)
    return state
  }

  /// A list of 40 sessions scrolled down, then a new session arriving at its top.
  ///
  /// - Returns: The window, its list, the session row read before the arrival and its pixels then.
  /// - Throws: When the list or a row to read is missing.
  static func scrolledWithArrival() async throws -> (
    host: ViewerHost, scroll: NSScrollView, retained: (key: String, value: CGRect), before: Data
  ) {
    var activity = sessions(40)
    let host = ViewerHost(state(activity), animatesArrivals: false)
    await render(host)
    let scroll = try #require(host.scrollViews.first)
    let unscrolledFrames = host.frames
    scroll.contentView.scroll(to: CGPoint(x: 0, y: scrolled))
    scroll.reflectScrolledClipView(scroll.contentView)
    await render(host)
    let viewport = host.frameFromTop(of: scroll)
    // Lazy rows that left the screen retain their old reports. Read only a row measured after scrolling.
    let retained = try #require(
      host.frames.first { key, frame in
        key.hasPrefix("session old-") && unscrolledFrames[key] != frame
          && frame.minY > viewport.minY + 40 && frame.maxY < viewport.maxY - 40
      }
    )
    let before = try host.pixels(in: retained.value)
    activity.days[0].sessions.insert(arrival, at: 0)
    host.show(state(activity))
    await render(host)
    return (host, scroll, retained, before)
  }

  private static func render(_ host: ViewerHost) async {
    for _ in 0..<6 {
      await afterQueuedWork()
      host.render()
    }
  }
}
