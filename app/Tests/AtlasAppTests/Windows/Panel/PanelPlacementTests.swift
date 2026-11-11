import Foundation
import Testing

@testable import AtlasApp

@Suite("PanelPlacement")
struct PanelPlacementTests {
  /// A 1440 by 900 screen clear of a 24-point menu bar.
  private let visible = CGRect(x: 0, y: 0, width: 1440, height: 876)
  /// A window for a 352-point card with 68 points of clear room at each side.
  private let size = CGSize(width: 488, height: 500)

  @Test("the panel hangs from the menu bar's lower edge, its card centred under Atlas's item")
  func thePanelHangsCentredUnderItsItem() {
    let anchor = CGRect(x: 700, y: 876, width: 24, height: 24)
    let frame = PanelPlacement.frame(size: size, under: anchor, in: visible, room: 68)
    #expect(frame == CGRect(x: 468, y: 376, width: 488, height: 500))
    #expect(frame.midX == anchor.midX)
    #expect(frame.maxY == anchor.minY, "its top is the menu bar's lower edge")
  }

  @Test("a panel whose card would cross the screen's side moves in, keeping 8 points clear of it")
  func thePanelStaysOnItsScreen() {
    let right = PanelPlacement.frame(
      size: size,
      under: CGRect(x: 1400, y: 876, width: 24, height: 24),
      in: visible,
      room: 68
    )
    #expect(right.maxX - 68 == 1432, "the card's right edge, 8 points from the screen's")
    let left = PanelPlacement.frame(
      size: size,
      under: CGRect(x: 10, y: 876, width: 24, height: 24),
      in: visible,
      room: 68
    )
    #expect(left.minX + 68 == 8, "the card's left edge, 8 points from the screen's")
  }

  @Test("a screen to the left of the main one places the panel on itself")
  func thePanelFollowsItsScreen() {
    let other = CGRect(x: -1920, y: 0, width: 1920, height: 1056)
    let frame = PanelPlacement.frame(
      size: size,
      under: CGRect(x: -40, y: 1056, width: 24, height: 24),
      in: other,
      room: 68
    )
    #expect(frame.maxX - 68 == -8)
    #expect(frame.maxY == 1056)
  }
}
