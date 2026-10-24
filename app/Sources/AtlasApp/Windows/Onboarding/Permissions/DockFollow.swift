import AtlasUI
import CoreGraphics
import Foundation

/// How the permission panel keeps to its place beside System Settings, one look at a time: a place a short way off,
/// as while the person drags System Settings, it takes at once, so it moves with the window; a place farther off, as
/// after a jump, it catches up with on ``Motion/LayerSpring/catchUp``, a spring with no bounce, never a snap. Pure, so
/// each look is tested on its own.
struct DockFollow {
  /// 24: how far off, in points, a place may be for the panel to take it at once.
  static let jump: CGFloat = 24
  /// 0.5: how close, in points, the panel must come for its catching up to end.
  static let landed: CGFloat = 0.5

  /// The panel's speed while it catches up, in points a second; zero at rest.
  private(set) var velocity = CGVector.zero
  /// Whether the panel is catching up with a place it was far from: until it lands, every look moves it on the spring.
  private(set) var isCatchingUp = false

  /// Where the panel goes at this look.
  ///
  /// - Parameters:
  ///   - current: Where the panel's origin is now.
  ///   - target: Where it belongs, beside System Settings.
  ///   - elapsed: The time since the last look, in seconds.
  /// - Returns: The panel's origin after this look.
  mutating func next(from current: CGPoint, toward target: CGPoint, after elapsed: TimeInterval) -> CGPoint {
    let distance = hypot(target.x - current.x, target.y - current.y)
    if !isCatchingUp, distance <= Self.jump {
      velocity = .zero
      return target
    }
    isCatchingUp = true
    let x = Self.spring(current.x - target.x, velocity.dx, after: elapsed)
    let y = Self.spring(current.y - target.y, velocity.dy, after: elapsed)
    velocity = CGVector(dx: x.velocity, dy: y.velocity)
    guard hypot(x.offset, y.offset) > Self.landed else {
      isCatchingUp = false
      velocity = .zero
      return target
    }
    return CGPoint(x: target.x + x.offset, y: target.y + y.offset)
  }

  /// One axis of a spring with no bounce, solved exactly, so a look of any length lands where the spring would be.
  ///
  /// - Parameters:
  ///   - offset: How far from its place the panel is on this axis.
  ///   - velocity: Its speed on this axis.
  ///   - elapsed: The time that passes, in seconds.
  /// - Returns: The offset and speed after it.
  private static func spring(
    _ offset: CGFloat,
    _ velocity: CGFloat,
    after elapsed: TimeInterval
  ) -> (offset: CGFloat, velocity: CGFloat) {
    let spring = Motion.LayerSpring.catchUp
    let rate = sqrt(spring.stiffness / spring.mass)
    let time = CGFloat(elapsed)
    let lead = velocity + CGFloat(rate) * offset
    let decay = CGFloat(exp(-rate * elapsed))
    return (
      (offset + lead * time) * decay,
      (velocity - CGFloat(rate) * lead * time) * decay
    )
  }
}
