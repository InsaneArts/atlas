import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Activity arrival scrolling", .serialized, AppKitState())
@MainActor
struct ActivityArrivalScrollTests {
  @Test("an action arriving in a scrolled Activity moment retains the reading position and has no arrival fade")
  func arrivalsLeaveScrolledContentInPlace() async throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    var state = PreviewStates.viewerActivity
    var moment = ViewerActivityMoment(
      id: "long moment",
      icon: .atlas,
      title: "Commands",
      time: "10:31",
      span: "40 actions",
      age: "now",
      isOpen: true
    )
    let steps =
      (0..<40)
      .map {
        ViewerActivityStep(id: "old-\($0)", kind: .command, words: ViewerStepWords("Action \($0)"), time: "now")
      }
    moment.steps = steps
    var activity = ViewerActivityState(
      meta: "",
      filters: PreviewStates.activityFilters(),
      days: [ViewerActivityDay(id: "today", title: "", moments: [moment])]
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
        key.hasPrefix("step old-") && unscrolledFrames[key] != frame
          && frame.minY > viewport.minY + 40 && frame.maxY < viewport.maxY - 40
      }
    )
    let before = try host.pixels(in: retained.value)
    let arrival = ViewerActivityStep(
      id: "new action",
      kind: .command,
      words: ViewerStepWords("Arrived now"),
      time: "now"
    )
    activity.days[0].moments[0].steps.insert(arrival, at: 0)
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
    let arrived = try #require(host.frames["step new action"])
    let settled = try #require(reference.frames["step new action"])
    #expect(try host.pixels(in: arrived) == reference.pixels(in: settled), "no arrival offset or fade was scheduled")
  }

  private static func render(_ host: ViewerHost) async {
    for _ in 0..<6 {
      await afterQueuedWork()
      host.render()
    }
  }
}
