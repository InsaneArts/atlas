import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Activity arrival scrolling", .serialized, AppKitState())
@MainActor
struct ActivityArrivalScrollTests {
  @Test("a session arriving in a scrolled Activity outline retains the reading position and has no arrival fade")
  func arrivalsLeaveScrolledContentInPlace() async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    var state = PreviewStates.viewerActivity
    let sessions =
      (0..<40)
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
    var activity = ViewerActivityState(
      meta: "",
      filters: PreviewStates.activityFilters(),
      days: [ViewerActivityDay(id: "today", title: "", sessions: sessions)]
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
        key.hasPrefix("session old-") && unscrolledFrames[key] != frame
          && frame.minY > viewport.minY + 40 && frame.maxY < viewport.maxY - 40
      }
    )
    let before = try host.pixels(in: retained.value)
    let arrival = ViewerOutlineSession(
      id: "new session",
      icon: nil,
      agent: "You",
      summary: "Arrived now",
      time: "10:31",
      runs: [ViewerOutlineRun(id: "new run", icon: .atlas, name: "Commands", summary: "Arrived now", time: "10:31")]
    )
    activity.days[0].sessions.insert(arrival, at: 0)
    state.pane = .activity(activity)
    let reference = ViewerHost(state, animatesArrivals: false)
    await Self.render(reference)
    Motion.Pose.hold(at: .zero)
    host.show(state)
    await Self.render(host)
    #expect(scroll.contentView.bounds.minY > 0)
    #expect(host.frames[retained.key] == retained.value, "the same words stay at the same screen position")
    #expect(try host.pixels(in: retained.value) == before)

    // Expose the just-inserted row before animation time advances: a scrolled arrival is already fully drawn.
    scroll.contentView.scroll(to: .zero)
    scroll.reflectScrolledClipView(scroll.contentView)
    await Self.render(host)
    await Self.render(reference)
    let arrived = try #require(host.frames["session new session"])
    let settled = try #require(reference.frames["session new session"])
    #expect(try host.pixels(in: arrived) == reference.pixels(in: settled), "no arrival offset or fade was scheduled")
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

  private static func render(_ host: ViewerHost) async {
    for _ in 0..<6 {
      await afterQueuedWork()
      host.render()
    }
  }
}
