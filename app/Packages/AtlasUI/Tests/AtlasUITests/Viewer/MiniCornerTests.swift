import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("MiniCorner")
struct MiniCornerTests {
  /// A stage 1,416 by 836, as the main window's at 1,440 by 900.
  static let stage = CGSize(width: 1416, height: 836)

  @Test("the small screen settles in the corner nearest where it is let go")
  func theSmallScreenSettlesInTheNearestCorner() {
    // Each quarter of the stage belongs to its corner.
    #expect(MiniCorner.nearest(to: CGPoint(x: 100, y: 100), in: Self.stage) == .topLeading)
    #expect(MiniCorner.nearest(to: CGPoint(x: 1300, y: 100), in: Self.stage) == .topTrailing)
    #expect(MiniCorner.nearest(to: CGPoint(x: 100, y: 700), in: Self.stage) == .bottomLeading)
    #expect(MiniCorner.nearest(to: CGPoint(x: 1300, y: 700), in: Self.stage) == .bottomTrailing)
    // On a middle line it goes right and down; past the stage's edge it still finds its corner.
    #expect(MiniCorner.nearest(to: CGPoint(x: 708, y: 418), in: Self.stage) == .bottomTrailing)
    #expect(MiniCorner.nearest(to: CGPoint(x: -50, y: -50), in: Self.stage) == .topLeading)
    // Dragged from the bottom right a little way, it goes back; across the middle, to the corner it was dragged to.
    let home = MiniCorner.bottomTrailing
    #expect(home.settling(after: CGSize(width: -200, height: -100), in: Self.stage) == .bottomTrailing)
    #expect(home.settling(after: CGSize(width: -900, height: 0), in: Self.stage) == .bottomLeading)
    #expect(home.settling(after: CGSize(width: -900, height: -500), in: Self.stage) == .topLeading)
    #expect(MiniCorner.topLeading.settling(after: CGSize(width: 900, height: 0), in: Self.stage) == .topTrailing)
  }

  @Test("the small screen sits 26 points from the two edges of its corner")
  func theSmallScreenSitsInItsCorner() {
    // 248 by 155, so its centre is 26 + 124 across and 26 + 77.5 down from its corner's edges.
    #expect(MiniCorner.bottomTrailing.centre(in: Self.stage) == CGPoint(x: 1416 - 150, y: 836 - 103.5))
    #expect(MiniCorner.topLeading.centre(in: Self.stage) == CGPoint(x: 150, y: 103.5))
    for corner in MiniCorner.allCases {
      #expect(MiniCorner.nearest(to: corner.centre(in: Self.stage), in: Self.stage) == corner, "\(corner)")
    }
    // Let go, it springs there; under Reduce Motion it moves at once.
    #expect(ViewerLayout.miniSnap(reduceMotion: false) == Motion.release)
    #expect(ViewerLayout.miniSnap(reduceMotion: true) == nil)
  }
}
