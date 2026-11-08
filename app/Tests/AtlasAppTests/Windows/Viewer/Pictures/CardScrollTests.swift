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
  nonisolated static let lists = ["activity", "apps"]

  @Test(
    "Activity and Apps each scroll the card's whole height under the floating picture, their end clear of it",
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

  @Test(
    "Apps keeps the card being read in Already open when another agent card arrives above it",
    .enabled(
      if: ProcessInfo.processInfo.environment["ATLAS_NATIVE_WINDOW_TESTS"] == "1",
      "requires the hidden helper graphical test host"
    )
  )
  func appCardsRetainTheirReadingPosition() async throws {
    let first = PreviewStates.viewerAppsMany
    var next = first
    var apps = try #require({ if case .apps(let apps) = first.pane { apps } else { nil } }())
    let mail = try #require(apps.cards.first { $0.name == "Mail" })
    // A full extra grid row changes the height above Already open at this window's column count.
    for index in 0..<4 {
      var added = try #require(apps.groups.first?.cards.first)
      added.id = 900 + index
      added.name = "New app \(index)"
      apps.groups[0].cards.append(added)
    }
    next.pane = .apps(apps)
    let host = ViewerHost(first, animatesArrivals: false, reduceMotion: true)
    host.presentNativeWindow()
    defer { host.closeNativeWindow() }
    let list = try #require(Self.list(in: host))
    list.contentView.scroll(to: NSPoint(x: 0, y: 600))
    list.reflectScrolledClipView(list.contentView)
    await Self.settle(host)
    let mailFrame = try #require(host.frames["app \(mail.id)"])
    let viewport = host.frameFromTop(of: list)
    list.contentView.scroll(to: NSPoint(x: 0, y: list.contentView.bounds.minY + mailFrame.minY - viewport.minY + 24))
    list.reflectScrolledClipView(list.contentView)
    await Self.settle(host)
    let before = try #require(host.frames["app \(mail.id)"])
    host.show(next)
    await Self.settle(host)
    let after = try #require(host.frames["app \(mail.id)"])
    #expect(abs(before.minY - after.minY) <= 1, "Mail moved from \(before) to \(after) while the earlier group grew")
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
    default: PreviewStates.viewerAppsMany
    }
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
      host.render()
    }
  }

  /// 6: how many turns a scroll takes to settle.
  static let passes = 6
}
