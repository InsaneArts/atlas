import SwiftUI

/// Durations and the animations built from them.
public enum Motion {
  /// One step of the duration scale. Its initializer is private, so only the scale's steps exist; an AppKit
  /// animation reads ``seconds``.
  public struct Duration: Hashable, Sendable {
    /// The step's length in seconds.
    public let seconds: TimeInterval

    private init(seconds: TimeInterval) {
      self.seconds = seconds
    }

    /// 0.1: a button press.
    static let press = Duration(seconds: 0.1)
    /// 0.2: a notice, an edge, content leaving.
    public static let fade = Duration(seconds: 0.2)
    /// 0.4: content arriving.
    static let entry = Duration(seconds: 0.4)
    /// 0.45: a window gliding to a new place on the screen and back.
    public static let glide = Duration(seconds: 0.45)
    /// 0.8: one breath of a status dot.
    static let pulse = Duration(seconds: 0.8)
    /// 0.9: one turn of the spinner.
    static let spin = Duration(seconds: 0.9)
  }

  /// A button's press: ease out over ``Duration/press``.
  static var press: Animation { .easeOut(duration: Duration.press.seconds) }

  /// A fade: ease out over ``Duration/fade``.
  static var fade: Animation { .easeOut(duration: Duration.fade.seconds) }

  /// An entry: ease out over ``Duration/entry``.
  static var entry: Animation { .easeOut(duration: Duration.entry.seconds) }

  /// A dot breathing in and out for as long as it shows.
  static var pulse: Animation {
    .easeInOut(duration: Duration.pulse.seconds)
      .repeatForever(autoreverses: true)
  }

  /// The spinner turning for as long as it shows.
  static var spin: Animation {
    .linear(duration: Duration.spin.seconds)
      .repeatForever(autoreverses: false)
  }

  /// A side column sliding in: a spring with no bounce.
  static var slide: Animation { .spring(response: slideResponse, dampingFraction: 1) }

  /// A position indicator moving, such as the page dots: a spring with a little bounce.
  static var indicator: Animation { .spring(response: indicatorResponse, dampingFraction: indicatorDamping) }

  /// The fade every transition takes under Reduce Motion, nothing moving: ease out over ``Duration/fade``. Each
  /// surface adopts it as it is redesigned; until then each keeps its own fallback through
  /// ``animation(_:reduceMotion:)`` or ``animation(_:reduceMotion:reduced:)``.
  static var reduced: Animation { .easeOut(duration: Duration.fade.seconds) }

  /// An animation, or none under Reduce Motion, so the change happens at once.
  ///
  /// - Parameters:
  ///   - base: The animation, such as ``press`` or ``fade``.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: `base`, or `nil` under Reduce Motion.
  static func animation(_ base: Animation, reduceMotion: Bool) -> Animation? {
    reduceMotion ? nil : base
  }

  /// An animation, or a gentler one under Reduce Motion, such as a fade in place of a slide.
  ///
  /// - Parameters:
  ///   - base: The animation, such as ``slide``.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  ///   - reduced: What runs under Reduce Motion, such as ``fade``.
  /// - Returns: `base`, or `reduced` under Reduce Motion.
  static func animation(_ base: Animation, reduceMotion: Bool, reduced: Animation) -> Animation {
    reduceMotion ? reduced : base
  }

  /// The slide's spring response, in seconds.
  private static let slideResponse = 0.35
  /// The indicator's spring response, in seconds.
  private static let indicatorResponse = 0.45
  /// The indicator's spring damping: 0.8 leaves a small settle.
  private static let indicatorDamping = 0.8
}
