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

  @Test("the picture gives Details its room on Details' own slide, never in one jump")
  func thePictureGivesDetailsItsRoomOnTheSlide() throws {
    var shown = PreviewStates.viewerDetails
    shown.screen.picture = try LayerPictureTests.picture(PictureColour.stage.rawValue)
    var hidden = shown
    hidden.details = nil
    // Settled, the square picture reaches past 1,000 points across without Details, and stops short of it beside it.
    #expect(try ViewerHost(hidden).colour(at: Self.besideDetails) == .stage)
    #expect(try ViewerHost(shown).colour(at: Self.besideDetails) == nil)
    #expect(try ViewerHost(shown).colour(at: Self.middle) == .stage)
    // Details' first frame: the picture still stands where it stood, about to shrink as the column slides in, rather
    // than leaving a gap at once while Details is still past the window's edge.
    let host = ViewerHost(hidden)
    host.show(shown)
    #expect(try host.colour(at: Self.besideDetails) == .stage)
    // And hiding it, the picture grows back as the column leaves: its first frame keeps the narrower picture.
    let back = ViewerHost(shown)
    back.show(hidden)
    #expect(try back.colour(at: Self.besideDetails) == nil)
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

  /// Below the top row and clear of the small screen: where the panes draw.
  static let panes = CGRect(x: 0, y: 60, width: 1100, height: 600)

  /// An Activity line's height.
  static let lineHeight: CGFloat = 38
  /// How far the list is scrolled before the switches.
  static let scrolled: CGFloat = 240
  /// Right of a square picture beside Details, and on one without it: 1,100 across, halfway down.
  static let besideDetails = CGPoint(x: 1100, y: 450)
  /// The middle of the picture beside Details.
  static let middle = CGPoint(x: 574, y: 450)

  /// The Activity tab with `lines` lines, enough to scroll.
  ///
  /// - Parameter lines: How many lines.
  /// - Returns: What the window shows.
  /// - Throws: Never; the samples always have Activity.
  static func activity(lines: Int) throws -> ViewerState {
    var state = PreviewStates.viewerActivity
    guard case .activity(var activity) = state.pane, let line = activity.groups.first?.lines.first else {
      return state
    }
    // Numbered from the oldest, so one more line is a new one at the top.
    let many = (0..<lines)
      .reversed()
      .map { index in
        ViewerActivityLine(
          id: "line \(index)",
          time: line.time,
          lead: .quiet,
          kind: line.kind,
          text: line.text,
          label: line.label
        )
      }
    activity.groups = [ViewerActivityGroup(id: "today", title: "Today", lines: many)]
    state.pane = .activity(activity)
    return state
  }
}
