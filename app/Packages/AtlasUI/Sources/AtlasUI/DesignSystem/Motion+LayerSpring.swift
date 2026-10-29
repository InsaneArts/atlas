import Foundation

extension Motion {
  /// A shared spring as Core Animation writes it: mass, stiffness and damping, for a `CASpringAnimation` in
  /// an AppKit layer, such as the menu bar badge's pop. It is made from the same duration and bounce as the
  /// SwiftUI spring it stands for, so a layer and a view that share a token share its feeling. Plain numbers,
  /// so the design system needs no Core Animation.
  public struct LayerSpring: Equatable, Sendable {
    /// The moving mass.
    public let mass: Double
    /// The spring's stiffness.
    public let stiffness: Double
    /// The damping that slows it.
    public let damping: Double

    /// A spring of a perceptual duration and bounce, as `spring(duration:bounce:)` defines them: a mass of 1 whose
    /// undamped swing takes the duration, so its stiffness is (2π / duration)², damped to a ratio of 1 − bounce (a
    /// negative bounce overdamps, to 1 / (1 + bounce)).
    ///
    /// - Parameters:
    ///   - duration: The perceptual duration, in seconds.
    ///   - bounce: The bounce, from −1 to 1; 0 settles without overshooting.
    init(duration: TimeInterval, bounce: Double) {
      let swing = 2 * Double.pi / duration
      let ratio = bounce < 0 ? 1 / (1 + bounce) : 1 - bounce
      mass = 1
      stiffness = swing * swing
      damping = 2 * ratio * sqrt(stiffness * mass)
    }

    /// A check, a badge or a symbol popping in: ``Motion/Springs/pop``, 0.35 seconds with bounce 0.15.
    public static let pop = LayerSpring(duration: Springs.pop.duration, bounce: Springs.pop.bounce)

    /// A window catching up with another it follows, such as the setup panel beside System Settings after a jump:
    /// ``Motion/Springs/arrive``, 0.25 seconds with no bounce.
    public static let catchUp = LayerSpring(duration: Springs.arrive.duration, bounce: Springs.arrive.bounce)

    /// The live picture's corners following it as it moves between the stage and the small screen, so the layer's
    /// corners and the view's clip land together: ``Motion/Springs/zoom``, 0.35 seconds with no bounce.
    public static let zoom = LayerSpring(duration: Springs.zoom.duration, bounce: Springs.zoom.bounce)
  }
}
