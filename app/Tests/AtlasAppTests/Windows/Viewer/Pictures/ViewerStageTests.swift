import AppKit
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("ViewerStage", .serialized, AppKitState())
@MainActor
struct ViewerStageTests {
  @Test("Activity shown again after the Screen is the pane already built, still scrolled where it was left")
  func aTabShownAgainIsThePaneAlreadyBuilt() throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let host = ViewerHost(try Self.activity(lines: 80))
    let list = try #require(host.scrollViews.first, "Activity has its list")
    let width = list.contentView.frame.width
    list.contentView.scroll(to: NSPoint(x: 0, y: Self.scrolled))
    list.reflectScrolledClipView(list.contentView)
    // SwiftUI hears a scroll on the run loop's later turns, as it would a wheel's: take a few, each waiting for nothing.
    let loop = RunLoop.main
    for _ in 0..<Self.settlingTurns {
      loop.run(mode: .default, before: .distantPast)
    }
    var steps = ["scrolled: \(Self.describe(list))"]
    host.show(PreviewStates.viewerWorking)
    steps.append("screen: \(Self.describe(list))")
    Self.settle(host)
    steps.append("screen settled: \(Self.describe(list))")
    host.show(try Self.activity(lines: 80))
    steps.append("activity: \(Self.describe(list))")
    Self.settle(host)
    steps.append("activity settled: \(Self.describe(list))")
    // Activity's list is all there is: nothing was built again, and nothing old is still leaving.
    #expect(host.scrollViews.count == 1)
    #expect(host.scrollViews.contains { $0 === list }, "Activity's list was built again")
    // Hidden, it kept its scroll bar, so it was never tiled again, and it stayed where it was read.
    #expect(list.contentView.frame.width == width, "the list was tiled again: \(steps)")
    #expect(list.contentView.bounds.origin.y == Self.scrolled, "the list lost its place: \(steps)")
  }

  @Test(
    "Activity hidden behind the Screen keeps its rows where they stood, with the small screen's room or without it",
    arguments: [true, false]
  )
  func activityHiddenBehindTheScreenKeepsItsRows(hasPicture: Bool) throws {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    // In a top corner, where the room the list keeps under the small screen stands above its first row.
    var activity = try Self.activity(lines: 80)
    activity.miniCorner = .topTrailing
    var screen = PreviewStates.viewerWorking
    screen.miniCorner = .topTrailing
    if !hasPicture {
      activity.mini = nil
      screen = PreviewStates.viewerNotReady
      screen.miniCorner = .topTrailing
    }
    let host = ViewerHost(activity)
    let list = try #require(host.scrollViews.first, "Activity has its list")
    let first = try #require(Self.top(of: Self.firstRow, in: list, host: host), "the first row is drawn")
    host.show(screen)
    Self.settle(host)
    // The small screen leaves with Activity, or never was, and the hidden list keeps the room it had.
    #expect(Self.top(of: Self.firstRow, in: list, host: host) == first, "the hidden list was laid out again")
  }

  /// The newest session's row, the first in the list.
  static let firstRow = "session line 79"

  /// How far below `list`'s top the row reporting `id` stands, in the window as it is drawn now.
  ///
  /// - Parameters:
  ///   - id: The row's reported name.
  ///   - list: The list's scroll view.
  ///   - host: The window.
  /// - Returns: The distance, or nil while the row is not drawn.
  static func top(of id: String, in list: NSScrollView, host: ViewerHost) -> CGFloat? {
    host.frames[id].map { $0.minY - host.frameFromTop(of: list).minY }
  }

  /// How `list` stands, for a failure to say: where it is scrolled, its content's frame, its visible width, and its
  /// scroll bar.
  ///
  /// - Parameter list: The list's scroll view.
  /// - Returns: The description.
  static func describe(_ list: NSScrollView) -> String {
    let scroller = list.verticalScroller
    return [
      "offset \(list.contentView.bounds.origin.y)",
      "content \(list.documentView?.frame ?? .zero)",
      "visible width \(list.contentView.frame.width)",
      "scroller \(list.hasVerticalScroller) hidden \(scroller?.isHidden ?? true)",
      "style \(list.scrollerStyle.rawValue) of \(NSScroller.preferredScrollerStyle.rawValue)",
      "autohides \(list.autohidesScrollers)",
    ]
    .joined(separator: ", ")
  }

  /// Lets every animation of the change just made finish, holding the moment well past the longest, then draws.
  ///
  /// - Parameter host: The window.
  static func settle(_ host: ViewerHost) {
    host.render()
    Motion.Pose.hold(at: settled)
    host.render()
    Motion.Pose.hold(at: .zero)
  }

  /// 5 seconds: well past the end of every animation a change runs.
  static let settled: TimeInterval = 5

  @Test("a new session shows at the top, while a list scrolled down holds it under a line and never moves")
  func aNewActionKeepsAScrolledListInPlace() throws {
    let top = ViewerHost(try Self.activity(lines: 80))
    let atTop = try #require(top.scrollViews.first)
    top.show(try Self.activity(lines: 81))
    top.render()
    // At the top it stays at the top, so the new session shows.
    #expect(atTop.contentView.bounds.origin.y == 0)
    #expect(top.frames["session line 80"] != nil)

    let host = ViewerHost(try Self.activity(lines: 80))
    let list = try #require(host.scrollViews.first)
    let unscrolled = host.frames
    list.contentView.scroll(to: NSPoint(x: 0, y: Self.scrolled))
    list.reflectScrolledClipView(list.contentView)
    // SwiftUI hears a scroll on the run loop's later turns, as it would a wheel's, and only then knows the row it
    // keeps in place: take a few, each waiting for nothing.
    let loop = RunLoop.main
    for _ in 0..<Self.settlingTurns {
      loop.run(mode: .default, before: .distantPast)
    }
    host.show(try Self.activity(lines: 80))
    let before = list.contentView.bounds.origin.y
    #expect(before == Self.scrolled, "the scroll did not hold: \(before)")
    host.render()
    let viewport = host.frameFromTop(of: list)
    // A session row measured after scrolling and well inside the list, which the person reads.
    let read = try #require(
      host.frames.first { key, frame in
        key.hasPrefix("session line ") && unscrolled[key] != frame
          && frame.minY > viewport.minY + 40 && frame.maxY < viewport.maxY - 40
      }
    )
    host.show(try Self.activity(lines: 81))
    // SwiftUI hears the change on the run loop's later turns, sooner or later as the Mac is busy: take a few, each
    // without waiting, until the line shows.
    for _ in 0..<Self.settlingTurns where host.frames["waiting line"] == nil {
      loop.run(mode: .default, before: .distantPast)
      host.render()
    }
    // Scrolled down, the new session waits above under a line, so nothing the person reads moves.
    #expect(list.contentView.bounds.origin.y == before, "the list stayed")
    #expect(host.frames[read.key] == read.value, "\(read.key) moved from \(read.value)")
    #expect(host.frames["session line 80"] == nil, "the new session waits")
    #expect(host.frames["waiting line"] != nil, "the line says it waits")
  }

  /// 10: the most run loop turns a change takes to settle in the list before it is measured.
  static let settlingTurns = 10

  /// How far the list is scrolled before the switches.
  static let scrolled: CGFloat = 240

  /// The Activity tab with `lines` sessions, each open on one folded run, enough to scroll.
  ///
  /// - Parameter lines: How many sessions.
  /// - Returns: What the window shows.
  /// - Throws: Never; the samples always have Activity.
  static func activity(lines: Int) throws -> ViewerState {
    var state = PreviewStates.viewerActivity
    guard case .activity(var activity) = state.pane else { return state }
    // Numbered from the oldest, newest first, so one more session is a new one at the top.
    let sessions = (0..<lines)
      .reversed()
      .map { index in
        ViewerOutlineSession(
          id: "line \(index)",
          icon: nil,
          agent: "You",
          summary: "Ran a command",
          time: "01:50",
          runs: [
            ViewerOutlineRun(
              id: "run \(index)",
              icon: .symbol(.terminal),
              name: "Commands",
              summary: "Ran a command",
              time: "01:50"
            )
          ]
        )
      }
    activity.days = [ViewerActivityDay(id: "today", title: "", sessions: sessions)]
    state.pane = .activity(activity)
    return state
  }
}
