import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("MiniCorner")
struct MiniCornerTests {
  /// A stage 1,416 by 836, as the main window's at 1,440 by 900.
  static let stage = CGSize(width: 1416, height: 836)
  /// The small screen in a stage that wide.
  static let mini = CGSize(width: 200, height: 125)

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
    #expect(home.settling(after: CGSize(width: -200, height: -100), in: Self.stage, mini: Self.mini) == .bottomTrailing)
    #expect(home.settling(after: CGSize(width: -900, height: 0), in: Self.stage, mini: Self.mini) == .bottomLeading)
    #expect(home.settling(after: CGSize(width: -900, height: -500), in: Self.stage, mini: Self.mini) == .topLeading)
    #expect(
      MiniCorner.topLeading.settling(after: CGSize(width: 900, height: 0), in: Self.stage, mini: Self.mini)
        == .topTrailing
    )
  }

  @Test("the small screen sits 12 points from the two edges of its corner")
  func theSmallScreenSitsInItsCorner() {
    // 200 by 125, so its centre is 12 + 100 across and 12 + 62.5 down from its corner's edges.
    #expect(MiniCorner.bottomTrailing.centre(in: Self.stage, mini: Self.mini) == CGPoint(x: 1416 - 112, y: 836 - 74.5))
    #expect(MiniCorner.topLeading.centre(in: Self.stage, mini: Self.mini) == CGPoint(x: 112, y: 74.5))
    for corner in MiniCorner.allCases {
      #expect(
        MiniCorner.nearest(to: corner.centre(in: Self.stage, mini: Self.mini), in: Self.stage) == corner,
        "\(corner)"
      )
    }
  }

  @Test("a thrown small screen lands in the corner its momentum heads for, going on at the speed it was thrown")
  func aThrownSmallScreenLandsWhereItWasThrown() {
    let home = MiniCorner.bottomTrailing
    // The pointer stopped a little way left, but the throw carries it across the middle: it lands bottom left.
    #expect(home.settling(after: CGSize(width: -200, height: 0), in: Self.stage, mini: Self.mini) == .bottomTrailing)
    #expect(home.settling(after: CGSize(width: -900, height: 0), in: Self.stage, mini: Self.mini) == .bottomLeading)
    // Thrown toward the corner it lands in, it keeps its speed as a share of the way left each second.
    let way =
      MiniCorner.bottomLeading
      .centre(in: Self.stage, mini: Self.mini).x - home.centre(in: Self.stage, mini: Self.mini).x + 200
    let velocity = home.landingVelocity(
      to: .bottomLeading,
      after: CGSize(width: -200, height: 0),
      velocity: CGSize(width: -2000, height: 0),
      in: Self.stage,
      mini: Self.mini
    )
    #expect(abs(velocity - 2000 / Double(-way)) < 1e-9)
    // Thrown away from it, or already there, it sets off from rest.
    #expect(
      home.landingVelocity(
        to: .bottomLeading,
        after: .zero,
        velocity: CGSize(width: 900, height: 0),
        in: Self.stage,
        mini: Self.mini
      )
        == 0
    )
    #expect(
      home.landingVelocity(
        to: home,
        after: .zero,
        velocity: CGSize(width: 900, height: 0),
        in: Self.stage,
        mini: Self.mini
      ) == 0
    )
    // It lands on a spring with a little bounce, the one bounce in the window; under Reduce Motion at once.
    #expect(abs(ViewerLayout.miniThrown.bounce - 0.1) < 1e-6)
    #expect(abs(ViewerLayout.miniThrown.duration - 0.4) < 1e-6)
    #expect(
      ViewerLayout.miniLanding(velocity: 3, reduceMotion: false)
        == .interpolatingSpring(ViewerLayout.miniThrown, initialVelocity: 3)
    )
    #expect(ViewerLayout.miniLanding(velocity: 3, reduceMotion: true) == nil)
  }
}
