import AppKit
import Testing

@testable import AtlasUI

@Suite("CardScrollPlace")
@MainActor
struct CardScrollPlaceTests {
  /// A scroll view over content taller than it, scrolled down to `offset`.
  ///
  /// - Parameter offset: How far it is scrolled.
  /// - Returns: The scroll view.
  static func scrolled(to offset: CGFloat) -> NSScrollView {
    let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
    scroll.documentView = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 3_000))
    scroll.contentView.scroll(to: NSPoint(x: 0, y: offset))
    scroll.reflectScrolledClipView(scroll.contentView)
    return scroll
  }

  @Test("a row the list moves as rows arrive above it is brought back to where it stood, however far the list moved")
  func aHeldRowIsBroughtBack() {
    let scroll = Self.scrolled(to: 620)
    let place = CardScrollPlace()
    place.scroll = scroll
    place.report("session a", top: -20)
    place.report("session b", top: 65)
    place.report("session c", top: 150)
    place.hold()
    // The Mac moved the list on by twice the new row's 85: the row read stands 85 higher than it did.
    place.report("session b", top: -20)
    #expect(scroll.contentView.bounds.origin.y == 535, "brought back by the 85 it moved")
  }

  @Test("with no row standing below the list's top, the one crossing it, such as a long open run, is held")
  func aRowCrossingTheTopIsHeld() {
    let scroll = Self.scrolled(to: 620)
    let place = CardScrollPlace()
    place.scroll = scroll
    place.report("run far above", top: -900)
    place.report("run read", top: -40)
    place.hold()
    place.report("run read", top: -125)
    #expect(scroll.contentView.bounds.origin.y == 535, "brought back by the 85 it moved")
  }

  @Test("a row the Mac already kept in place, or one reported with nothing held, moves nothing")
  func nothingMovesWithoutADrift() {
    let scroll = Self.scrolled(to: 535)
    let place = CardScrollPlace()
    place.scroll = scroll
    place.report("session b", top: 65)
    place.hold()
    place.report("session b", top: 65)
    place.report("session c", top: 10)
    #expect(scroll.contentView.bounds.origin.y == 535)
  }
}
