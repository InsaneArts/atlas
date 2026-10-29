import AppKit
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("ViewerStage", .serialized, AppKitState())
@MainActor
struct ViewerStageTests {
  @Test("a tab shown again is the pane already built, still scrolled where it was left")
  func aTabShownAgainIsThePaneAlreadyBuilt() throws {
    let host = ViewerHost(try Self.activity(lines: 80))
    let list = try #require(host.scrollViews.first, "Activity has its list")
    list.contentView.scroll(to: NSPoint(x: 0, y: Self.scrolled))
    list.reflectScrolledClipView(list.contentView)
    host.show(PreviewStates.viewerApps)
    host.show(try Self.activity(lines: 80))
    // Activity's list and Apps' are all there is: nothing was built again, and nothing old is still leaving.
    #expect(host.scrollViews.count == 2)
    #expect(host.scrollViews.contains { $0 === list }, "Activity's list was built again")
    #expect(list.contentView.bounds.origin.y == Self.scrolled, "the list lost its place")
  }

  @Test("a switch's first frame, and a switch reversed at once, still show the pane left whole: never a blank stage")
  func aSwitchNeverShowsABlankStage() throws {
    let activity = try Self.activity(lines: 80)
    let reference = ViewerHost(activity)
    let settled = try reference.pixels(in: Self.panes)
    let host = ViewerHost(activity)
    host.show(PreviewStates.viewerApps)
    host.show(activity)
    host.show(PreviewStates.viewerApps)
    // As the switch begins the old pane still shows whole, the new one not yet in: the stage is never empty.
    #expect(try host.pixels(in: Self.panes) == settled, "Activity left the stage at once")
    host.show(activity)
    // Turned back at once, Activity never left: it shows whole, and Apps has not come in.
    #expect(try host.pixels(in: Self.panes) == settled, "turning back left a gap")
    #expect(host.scrollViews.count == 2, "a pane was built again")
  }

  @Test("a new action arrives at the top and shows there, while a list scrolled down keeps the line read in place")
  func aNewActionKeepsAScrolledListInPlace() throws {
    let top = ViewerHost(try Self.activity(lines: 80))
    let atTop = try #require(top.scrollViews.first)
    top.show(try Self.activity(lines: 81))
    // At the top it stays at the top, so the new line shows.
    #expect(atTop.contentView.bounds.origin.y == 0)

    let host = ViewerHost(try Self.activity(lines: 80))
    let list = try #require(host.scrollViews.first)
    list.contentView.scroll(to: NSPoint(x: 0, y: Self.scrolled))
    list.reflectScrolledClipView(list.contentView)
    // SwiftUI hears a scroll on the run loop's next turn, as it would a wheel's: take that turn, waiting for nothing.
    let loop = RunLoop.main
    loop.run(mode: .default, before: .distantPast)
    host.show(try Self.activity(lines: 80))
    let before = list.contentView.bounds.origin.y
    #expect(before == Self.scrolled, "the scroll did not hold: \(before)")
    host.show(try Self.activity(lines: 81))
    // Scrolled down, the list moves on by the new line's height, so the line the person reads holds its place.
    let after = list.contentView.bounds.origin.y
    #expect(after == Self.scrolled + Self.lineHeight, "at \(after)")
  }

  /// In the card, under its header and clear of the small screen: where the panes draw.
  static let panes = CGRect(x: 260, y: 60, width: 840, height: 600)

  /// An Activity step's height.
  static let lineHeight: CGFloat = 30
  /// How far the list is scrolled before the switches.
  static let scrolled: CGFloat = 240

  /// The Activity tab with one open moment of `lines` steps, enough to scroll.
  ///
  /// - Parameter lines: How many steps.
  /// - Returns: What the window shows.
  /// - Throws: Never; the samples always have Activity.
  static func activity(lines: Int) throws -> ViewerState {
    var state = PreviewStates.viewerActivity
    guard case .activity(var activity) = state.pane,
      let moment = activity.items.first,
      case .step(let step)? = activity.items.first(where: { if case .step = $0 { true } else { false } })
    else {
      return state
    }
    // Numbered from the oldest, so one more step is a new one at the top.
    let many = (0..<lines)
      .reversed()
      .map { index in
        var line = step
        line.id = "line \(index)"
        line.mark = .quiet
        line.shot = nil
        line.isLast = index == 0
        return ViewerActivityItem.step(line)
      }
    activity.items = [moment] + many
    state.pane = .activity(activity)
    return state
  }
}
