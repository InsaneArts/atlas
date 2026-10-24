import SwiftUI

/// Durations and the animations built from them. v3's motion (D14) is written as springs, `spring(duration:bounce:)`,
/// beside the older ease-out curves, which stay until their surfaces are redesigned. Under Reduce Motion an
/// animation becomes ``reduced`` through ``animation(_:reduceMotion:reduced:)``, a moving transition becomes a fade
/// through ``transition(_:reduceMotion:)``, and a loop stops through ``loops(reduceMotion:)``: colour and opacity
/// changes of at most 0.2 seconds are kept, and movement, scale, blur and loops are dropped (v3's policy).
public enum Motion {
  /// One step of the duration scale, shortest first. Its initializer is private, so only the scale's steps exist; an
  /// AppKit animation reads ``seconds``. Steps that play different roles may share a length.
  public struct Duration: Hashable, Sendable {
    /// The step's length in seconds.
    public let seconds: TimeInterval

    private init(seconds: TimeInterval) {
      self.seconds = seconds
    }

    /// 0.08: how long a drawn check waits after its disc before it draws (v3 S3, X11).
    static let drawDelay = Duration(seconds: 0.08)
    /// 0.1: a button press.
    static let press = Duration(seconds: 0.1)
    /// 0.12: a fill or a reveal answering the pointer arriving (v3 `Motion.hover`, in).
    static let hoverIn = Duration(seconds: 0.12)
    /// 0.15: an alert leaving, fading and shrinking a little (v3 A6).
    static let dismiss = Duration(seconds: 0.15)
    /// 0.18: a chip, a row, a slot or a toast leaving (v3 `Motion.leave`).
    static let leave = Duration(seconds: 0.18)
    /// 0.2: a notice, an edge, content leaving.
    public static let fade = Duration(seconds: 0.2)
    /// 0.2: a fill or a reveal going back as the pointer leaves (v3 `Motion.hover`, out).
    static let hoverOut = Duration(seconds: 0.2)
    /// 0.2: the fade every transition takes under Reduce Motion, the longest v3's policy keeps.
    static let reduced = Duration(seconds: 0.2)
    /// 0.3: a check drawing itself along its stroke (v3 S3, X11: `trim` 0 → 1).
    static let draw = Duration(seconds: 0.3)
    /// 0.36: an alert shaking once when its command is refused (v3 A4).
    static let shake = Duration(seconds: 0.36)
    /// 0.4: content arriving.
    static let entry = Duration(seconds: 0.4)
    /// 0.4: how long a spinner or a placeholder waits before it shows, so a quick answer never flashes one (v3
    /// `Duration.appearDelay`, R12).
    static let appearDelay = Duration(seconds: 0.4)
    /// 0.45: a window gliding to a new place on the screen and back.
    public static let glide = Duration(seconds: 0.45)
    /// 0.5: a window gliding on ``Motion/glideSoftCurve``, which starts at once and lands softly (v3 `Duration.glide`,
    /// S7); it replaces ``glide`` when the setup window's glide is redesigned.
    public static let glideSoft = Duration(seconds: 0.5)
    /// 0.7: how long a done alert stays, its check and Recovered showing, before it closes (v3 A5).
    public static let doneHold = Duration(seconds: 0.7)
    /// 0.8: one breath of a status dot.
    static let pulse = Duration(seconds: 0.8)
    /// 0.9: one turn of the spinner.
    static let spin = Duration(seconds: 0.9)
    /// 1.6: one breath of the live badge (v3 `Duration.breathe`).
    static let breathe = Duration(seconds: 1.6)
    /// 2.4: how long a toast stays (v3 `Duration.toast`).
    static let toast = Duration(seconds: 2.4)
  }

  /// The springs behind the animations, kept as values so their duration and bounce can be read back.
  enum Springs {
    /// A button coming back up and an alert settling: 0.3 seconds, bounce 0.2.
    static let release = Spring(duration: 0.3, bounce: 0.2)
    /// A tint, a rim, a colour or a row moving: 0.3 seconds, no bounce.
    static let state = Spring(duration: 0.3, bounce: 0)
    /// A chip, a row, a slot or a toast appearing: 0.3 seconds, bounce 0.15.
    static let arrive = Spring(duration: 0.3, bounce: 0.15)
    /// One setup screen giving way to the next: 0.42 seconds, no bounce.
    static let page = Spring(duration: 0.42, bounce: 0)
    /// A check, a badge or a symbol popping in: 0.35 seconds, bounce 0.3.
    static let pop = Spring(duration: 0.35, bounce: 0.3)
    /// A tab's thumb and its pane: 0.32 seconds, bounce 0.12.
    static let tab = Spring(duration: 0.32, bounce: 0.12)
    /// A side column sliding in: 0.35 seconds, no bounce, the curve of today's ``Motion/slide``.
    static let slide = Spring(duration: 0.35, bounce: 0)
    /// A window or a pane changing height: 0.35 seconds, no bounce.
    static let resize = Spring(duration: 0.35, bounce: 0)
    /// A screenshot zooming, or the screen turning into the small screen and back: 0.4 seconds, bounce 0.12.
    static let zoom = Spring(duration: 0.4, bounce: 0.12)
    /// A position indicator moving: 0.45 seconds, bounce 0.2, the curve of today's ``Motion/indicator``.
    static let indicator = Spring(duration: 0.45, bounce: 0.2)
  }

  /// The setup window's glide curve: it starts at once and lands softly (v3 S7, X13), as a cubic Bézier's two
  /// control points, for `CAMediaTimingFunction(controlPoints:)`.
  public static let glideSoftCurve = (x1: Float(0.25), y1: Float(1), x2: Float(0.5), y2: Float(1))

  /// A button's press: ease out over ``Duration/press``.
  static var press: Animation { .easeOut(duration: Duration.press.seconds) }

  /// A button's release and an alert's settle: ``Springs/release``.
  static var release: Animation { .spring(Springs.release) }

  /// A tint, a rim, a colour or a row moving to its new state: ``Springs/state``.
  static var state: Animation { .spring(Springs.state) }

  /// A chip, a row, a slot or a toast appearing: ``Springs/arrive``.
  static var arrive: Animation { .spring(Springs.arrive) }

  /// The same leaving: ease in over ``Duration/leave``.
  static var leave: Animation { .easeIn(duration: Duration.leave.seconds) }

  /// A setup screen giving way to the next: ``Springs/page``.
  static var page: Animation { .spring(Springs.page) }

  /// A check, a badge or a symbol popping in: ``Springs/pop``.
  static var pop: Animation { .spring(Springs.pop) }

  /// A tab's thumb and its pane: ``Springs/tab``.
  static var tab: Animation { .spring(Springs.tab) }

  /// A window or a pane changing height: ``Springs/resize``.
  static var resize: Animation { .spring(Springs.resize) }

  /// A screenshot zooming, or the screen turning into the small screen: ``Springs/zoom``.
  static var zoom: Animation { .spring(Springs.zoom) }

  /// A hover fill or reveal: ease out over ``Duration/hoverIn`` as the pointer arrives and over ``Duration/hoverOut``
  /// as it leaves. Only colour and opacity change on hover, so Reduce Motion keeps it.
  ///
  /// - Parameter isHovered: Whether the pointer is over the view now.
  /// - Returns: The animation for the change to `isHovered`.
  static func hover(_ isHovered: Bool) -> Animation {
    .easeOut(duration: (isHovered ? Duration.hoverIn : Duration.hoverOut).seconds)
  }

  /// A check drawing itself: ease out over ``Duration/draw``, after ``Duration/drawDelay``, so its disc lands first.
  /// It moves along a stroke, so under Reduce Motion the check appears whole instead.
  static var draw: Animation {
    .easeOut(duration: Duration.draw.seconds)
      .delay(Duration.drawDelay.seconds)
  }

  /// Something waiting to show, such as a spinner: a fade over ``Duration/fade`` after ``Duration/appearDelay``, so a
  /// quick answer never flashes it (v3 R12, S9). Only opacity changes, so Reduce Motion keeps it.
  static var appear: Animation {
    .easeOut(duration: Duration.fade.seconds)
      .delay(Duration.appearDelay.seconds)
  }

  /// An alert leaving: ease in over ``Duration/dismiss`` (v3 A6). It shrinks as it fades, so under Reduce Motion it
  /// becomes ``reduced``.
  static var dismiss: Animation { .easeIn(duration: Duration.dismiss.seconds) }

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

  /// A side column sliding in: a spring with no bounce. v3's ``Springs/slide`` draws the same curve and takes over
  /// when its surface is redesigned.
  static var slide: Animation { .spring(response: slideResponse, dampingFraction: 1) }

  /// A position indicator moving, such as the page dots: a spring with a little bounce. v3's ``Springs/indicator``
  /// draws the same curve and takes over when its surface is redesigned.
  static var indicator: Animation { .spring(response: indicatorResponse, dampingFraction: indicatorDamping) }

  /// The fade every transition takes under Reduce Motion, nothing moving: ease out over ``Duration/reduced``. Each
  /// surface adopts it as it is redesigned; until then each keeps its own fallback through
  /// ``animation(_:reduceMotion:)`` or ``animation(_:reduceMotion:reduced:)``.
  static var reduced: Animation { .easeOut(duration: Duration.reduced.seconds) }

  /// An animation, or none under Reduce Motion, so the change happens at once.
  ///
  /// - Parameters:
  ///   - base: The animation, such as ``press`` or ``fade``.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: `base`, or `nil` under Reduce Motion.
  static func animation(_ base: Animation, reduceMotion: Bool) -> Animation? {
    reduceMotion ? nil : base
  }

  /// An animation, or a gentler one under Reduce Motion, such as ``reduced`` in place of a spring.
  ///
  /// - Parameters:
  ///   - base: The animation, such as ``slide``.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  ///   - reduced: What runs under Reduce Motion, such as ``reduced`` or ``fade``.
  /// - Returns: `base`, or `reduced` under Reduce Motion.
  static func animation(_ base: Animation, reduceMotion: Bool, reduced: Animation) -> Animation {
    reduceMotion ? reduced : base
  }

  /// A transition that moves, or a plain fade under Reduce Motion, so nothing slides, scales or blurs.
  ///
  /// - Parameters:
  ///   - moving: The transition, such as `.move(edge: .trailing)`.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: `moving`, or `.opacity` under Reduce Motion.
  static func transition(_ moving: AnyTransition, reduceMotion: Bool) -> AnyTransition {
    reduceMotion ? .opacity : moving
  }

  /// Whether a loop runs, such as a breath or a spinning ring: never under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: False under Reduce Motion.
  static func loops(reduceMotion: Bool) -> Bool {
    !reduceMotion
  }

  /// The slide's spring response, in seconds.
  private static let slideResponse = 0.35
  /// The indicator's spring response, in seconds.
  private static let indicatorResponse = 0.45
  /// The indicator's spring damping: 0.8 leaves a small settle.
  private static let indicatorDamping = 0.8
}
