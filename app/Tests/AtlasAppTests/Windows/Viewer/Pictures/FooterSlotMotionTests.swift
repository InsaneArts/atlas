import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Footer slot motion", .serialized, AppKitState())
@MainActor
struct FooterSlotMotionTests {
  @Test("Stop fades in only once the buttons beside it have moved aside")
  func stopComesAfterItsNeighboursMove() throws {
    let settled = try Self.brightness(of: PreviewStates.viewerWorking, at: Self.stopMark)
    let moving = try Self.brightness(
      from: PreviewStates.viewerControl,
      to: PreviewStates.viewerWorking,
      at: Self.neighboursMoving,
      of: Self.stopMark
    )
    #expect(settled > Self.lit, "Stop's mark is drawn once settled")
    #expect(moving < Self.unlit, "Stop is drawn while its neighbours still slide over its place")
  }

  @Test("the buttons beside Stop hold still until it has faded")
  func neighboursWaitForStopToLeave() throws {
    let settled = try Self.brightness(of: PreviewStates.viewerWorking, at: Self.soundMark)
    let leaving = try Self.brightness(
      from: PreviewStates.viewerWorking,
      to: PreviewStates.viewerControl,
      at: Self.stopLeaving,
      of: Self.soundMark
    )
    #expect(settled > Self.lit, "the sound button's mark is drawn once settled")
    #expect(leaving > Self.lit, "the sound button moved while Stop was still fading")
  }

  /// The centre of Stop's square in a standard window, in points from its top left.
  static let stopMark = CGPoint(x: 1215.5, y: 870)
  /// A point on the sound button's mark while the agent works, in points from the window's top left.
  static let soundMark = CGPoint(x: 1161.5, y: 870)
  /// 0.15 seconds: Stop's neighbours are on their way, and it has not begun to fade in.
  static let neighboursMoving: TimeInterval = 0.15
  /// 0.08 seconds: Stop is still fading out, and nothing beside it has moved.
  static let stopLeaving: TimeInterval = 0.08
  /// Brightness a light mark has on the dark card.
  static let lit: CGFloat = 0.7
  /// Brightness at most where no mark is drawn on the dark card.
  static let unlit: CGFloat = 0.4

  /// How bright `point` is in the settled dark window showing `state`.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - point: The point, in points from its top left.
  /// - Returns: Its brightness.
  /// - Throws: When nothing was drawn.
  static func brightness(of state: ViewerState, at point: CGPoint) throws -> CGFloat {
    let host = ViewerHost(state, animatesArrivals: false, appearance: NSAppearance(named: .darkAqua))
    host.render()
    return try host.brightness(at: [point])[0]
  }

  /// How bright `point` is `moment` seconds after the dark window goes from `first` to `next`.
  ///
  /// - Parameters:
  ///   - first: What the window shows before the change.
  ///   - next: What it shows after.
  ///   - moment: The moment held, in seconds since the change.
  ///   - point: The point, in points from the window's top left.
  /// - Returns: Its brightness.
  /// - Throws: When nothing was drawn.
  static func brightness(
    from first: ViewerState,
    to next: ViewerState,
    at moment: TimeInterval,
    of point: CGPoint
  ) throws -> CGFloat {
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: -5)
    defer { Motion.Pose.hold(at: previous) }
    let host = ViewerHost(first, appearance: NSAppearance(named: .darkAqua))
    host.render()
    Motion.Pose.hold(at: .zero)
    host.render()
    host.show(next)
    host.render()
    Motion.Pose.hold(at: moment)
    host.render()
    return try host.brightness(at: [point])[0]
  }
}
