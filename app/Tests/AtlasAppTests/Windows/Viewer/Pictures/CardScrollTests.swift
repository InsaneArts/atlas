import AppKit
import AsyncSupportTesting
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("CardScroll", .serialized, AppKitState())
@MainActor
struct CardScrollTests {
  /// The lists in the card, each long enough to scroll, with the small screen in the bottom-trailing corner.
  nonisolated static let lists = ["activity", "screenshots", "apps"]

  @Test(
    "Activity, Screenshots and Apps each scroll the card's whole height under the floating picture, their end clear of it",
    arguments: lists
  )
  func eachListScrollsUnderTheFloatingPicture(list name: String) async throws {
    let host = ViewerHost(try Self.state(name))
    let list = try #require(Self.list(in: host), "\(name) has no list")
    let frame = host.frameFromTop(of: list)
    // The list runs down to the card's footer, past the small screen's foot: nothing ends early above it.
    #expect(frame.maxY >= Self.pipBottom, "\(name) ends at \(frame.maxY), above the small screen")
    await Self.scrollToEnd(list, in: host)
    // Scrolled to its end, the band the small screen floats in holds nothing of the list beside it: its last row
    // stands clear above, and nothing fades.
    let points = stride(from: frame.maxY - Self.band + 8, to: frame.maxY - 4, by: 16)
      .flatMap { y in
        // Short of the small screen's shadow.
        stride(from: frame.minX + 4, to: Self.pipLeading - 48, by: 32)
          .map { CGPoint(x: $0, y: y) }
      }
    let ground = try host.brightness(at: points)
    let first = try #require(ground.first)
    let odd = zip(points, ground)
      .filter { abs($0.1 - first) >= 0.01 }
      .map(\.0)
    #expect(odd.isEmpty, "\(name) draws in the small screen's band at its end, at \(odd.prefix(4)) in \(frame)")
  }

  @Test("a step to a tile under the picture scrolls it into view clear of the picture, only as far as it must")
  func aStepScrollsTheSelectedTileClearOfThePicture() async throws {
    let host = ViewerHost(PreviewStates.viewerShotsMany)
    let list = try #require(Self.list(in: host))
    #expect(list.contentView.bounds.origin.y == 0)
    // The selected tile is the very last, in the bottom-trailing corner, under the small screen.
    host.show(Self.stepped(PreviewStates.viewerShotsMany))
    await Self.settle(host)
    let end = try #require(list.documentView).frame.height - list.contentView.bounds.height
    let offset = list.contentView.bounds.origin.y
    // Its room, the tile and the small screen's band under it, now stands whole at the list's foot.
    #expect(offset > end - 16 && offset <= end, "scrolled to \(offset) of \(end)")
  }

  @Test("a step to a tile already in view leaves the list where it is")
  func aStepToATileInViewLeavesTheList() async throws {
    var first = PreviewStates.viewerShotsMany
    guard case .screenshots(var shots) = first.pane else { return }
    for day in shots.days.indices {
      for tile in shots.days[day].shots.indices {
        shots.days[day].shots[tile].isSelected = day == 0 && tile == 0
      }
    }
    first.pane = .screenshots(shots)
    let host = ViewerHost(first)
    let list = try #require(Self.list(in: host))
    host.show(Self.stepped(first))
    await Self.settle(host)
    #expect(list.contentView.bounds.origin.y == 0)
  }

  /// 834: the small screen's foot, from the window's top: 12 over the card's footer, 46 high, 8 in from the bottom.
  static let pipBottom: CGFloat = 834
  /// 1220: the small screen's leading edge, 200 wide and 12 in from the card's trailing edge, 8 in from the window's.
  static let pipLeading: CGFloat = 1220
  /// 149: the band the small screen floats in at the list's end, its 125 height and 12 on each side.
  static let band: CGFloat = 149

  /// A list's sample, long enough to scroll.
  ///
  /// - Parameter name: Which list.
  /// - Returns: What the window shows.
  /// - Throws: Never; the samples always have Activity.
  static func state(_ name: String) throws -> ViewerState {
    switch name {
    case "activity": try ViewerStageTests.activity(lines: 80)
    case "screenshots": PreviewStates.viewerShotsMany
    default: PreviewStates.viewerAppsMany
    }
  }

  /// `state` with one more keyboard step made.
  ///
  /// - Parameter state: A Screenshots sample.
  /// - Returns: The sample, stepped.
  static func stepped(_ state: ViewerState) -> ViewerState {
    var stepped = state
    guard case .screenshots(var shots) = stepped.pane else { return state }
    shots.scrollRequest += 1
    stepped.pane = .screenshots(shots)
    return stepped
  }

  /// The card's list: its tallest scroll view.
  ///
  /// - Parameter host: The window.
  /// - Returns: The list's scroll view, if any.
  static func list(in host: ViewerHost) -> NSScrollView? {
    host.scrollViews.max { $0.frame.height < $1.frame.height }
  }

  /// Scrolls `list` to its very end, again as a lazy list grows while it builds the rows it now shows.
  ///
  /// - Parameters:
  ///   - list: The list's scroll view.
  ///   - host: The window.
  static func scrollToEnd(_ list: NSScrollView, in host: ViewerHost) async {
    for _ in 0..<Self.passes {
      let height = list.documentView?.frame.height ?? .zero
      list.contentView.scroll(to: NSPoint(x: 0, y: max(height - list.contentView.bounds.height, 0)))
      list.reflectScrolledClipView(list.contentView)
      await settle(host)
    }
  }

  /// Lets the work queued on the main actor run and lays the window out again, a few times over, so SwiftUI hears a
  /// scroll and builds and scrolls as it would on screen.
  ///
  /// - Parameter host: The window.
  static func settle(_ host: ViewerHost) async {
    for _ in 0..<Self.passes {
      await afterQueuedWork()
      host.layOut()
    }
  }

  /// 6: how many turns a scroll takes to settle.
  static let passes = 6
}
