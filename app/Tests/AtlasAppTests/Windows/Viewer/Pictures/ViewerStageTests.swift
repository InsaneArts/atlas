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
    let host = ViewerHost(try Self.activity(lines: 80))
    let list = try #require(host.scrollViews.first, "Activity has its list")
    list.contentView.scroll(to: NSPoint(x: 0, y: Self.scrolled))
    list.reflectScrolledClipView(list.contentView)
    host.show(PreviewStates.viewerWorking)
    host.show(try Self.activity(lines: 80))
    // Activity's list is all there is: nothing was built again, and nothing old is still leaving.
    #expect(host.scrollViews.count == 1)
    #expect(host.scrollViews.contains { $0 === list }, "Activity's list was built again")
    #expect(list.contentView.bounds.origin.y == Self.scrolled, "the list lost its place")
  }

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
