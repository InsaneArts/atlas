import SwiftUI

/// Durations and the animations built from them: springs, `spring(duration:bounce:)`, alongside ease-out
/// curves for changes that should settle without bounce. Under Reduce Motion an animation becomes ``reduced``
/// through ``animation(_:reduceMotion:reduced:)``, a moving transition becomes a fade through
/// ``transition(_:reduceMotion:)``, and a loop stops through ``loops(reduceMotion:)``: colour and opacity
/// changes of at most 0.2 seconds are kept, and movement, scale, blur and loops are dropped.
public enum Motion {
  /// One step of the duration scale, shortest first. Its initializer is private, so only the scale's steps exist; an
  /// AppKit animation reads ``seconds``. Steps that play different roles may share a length.
  public struct Duration: Hashable, Sendable {
    /// The step's length in seconds.
    public let seconds: TimeInterval

    private init(seconds: TimeInterval) {
      self.seconds = seconds
    }

    /// 0.08: how long a drawn check waits after its disc before it draws.
    static let drawDelay = Duration(seconds: 0.08)
    /// 0.1: a text link's press.
    static let press = Duration(seconds: 0.1)
    /// 0.1: a window's content fading out before the window glides to a new place and size, so nothing
    /// reflows on the way; AppKit reads it.
    public static let contentOut = Duration(seconds: 0.1)
    /// 0.1: a tab's pane fading out as another takes its place, eased in.
    static let swapOut = Duration(seconds: 0.1)
    /// 0.12: a fill or a reveal answering the pointer arriving.
    static let hoverIn = Duration(seconds: 0.12)
    /// 0.12: a pane leaving as another takes its place.
    static let crossfadeOut = Duration(seconds: 0.12)
    /// 0.12: a step finishing: its number shrinking away, and how long the next step waits before it lights
    /// up.
    static let step = Duration(seconds: 0.12)
    /// 0.12: a setup page fading out as the next comes, eased in.
    static let pageOut = Duration(seconds: 0.12)
    /// 0.12: a button's pressed fill easing back once it is let go; the fill itself lands on mouse-down with no
    /// animation, as AppKit's buttons darken.
    static let pressRelease = Duration(seconds: 0.12)
    /// 0.15: a pane's crossfade under Reduce Motion, both ways.
    static let crossfadeReduced = Duration(seconds: 0.15)
    /// 0.15: an alert leaving, fading and shrinking a little, or a caption fading away.
    static let dismiss = Duration(seconds: 0.15)
    /// 0.15: how long what follows a success waits, such as Continue after the last check, which the setup window's
    /// controller waits on its clock, so Continue takes no press before it lights up.
    public static let followUp = Duration(seconds: 0.15)
    /// 0.15: a tab's pane fading in over the last, eased out, with no delay.
    static let swapIn = Duration(seconds: 0.15)
    /// 0.15: changing words swapping in place by opacity, the old fading as the new come.
    static let textSwap = Duration(seconds: 0.15)
    /// 0.18: a setup page fading in over the last, eased out, with no delay.
    static let pageIn = Duration(seconds: 0.18)
    /// 0.18: a chip, a row, a slot or a toast leaving.
    static let leave = Duration(seconds: 0.18)
    /// 0.2: a notice, an edge, content leaving.
    public static let fade = Duration(seconds: 0.2)
    /// 0.2: a fill or a reveal going back as the pointer leaves.
    static let hoverOut = Duration(seconds: 0.2)
    /// 0.2: the fade every transition takes under Reduce Motion, the maximum duration retained for reduced
    /// motion.
    static let reduced = Duration(seconds: 0.2)
    /// 0.2: the menu bar badge's colour and shape crossfading from one state to the next.
    public static let crossfade = Duration(seconds: 0.2)
    /// 0.25: the menu bar orbit's dim fading in or out.
    public static let dim = Duration(seconds: 0.25)
    /// 0.25: a window's height easing to fit its pane on ``Motion/resizeCurve``, its top edge fixed;
    /// AppKit reads it.
    public static let resize = Duration(seconds: 0.25)
    /// 0.3: how long a working note waits before it shows, so work that ends sooner never flashes it; a problem shows
    /// at once.
    public static let noteDelay = Duration(seconds: 0.3)
    /// 0.3: a check drawing itself along its stroke.
    static let draw = Duration(seconds: 0.3)
    /// 0.3: a piece moving to a new state, ``Motion/state``'s length; AppKit reads it for the menu bar
    /// panel's height easing to fit.
    public static let state = Duration(seconds: 0.3)
    /// 0.36: an alert shaking once when its command is refused.
    static let shake = Duration(seconds: 0.36)
    /// 0.4: how long a spinner or a placeholder waits before it shows, so a quick answer never flashes one.
    public static let appearDelay = Duration(seconds: 0.4)
    /// 0.5: a window gliding on ``Motion/glideSoftCurve``, which starts at once and lands softly.
    public static let glideSoft = Duration(seconds: 0.5)
    /// 0.6: the least time work shows once it shows, such as a check: shorter reads as a flicker, and 0.6 seconds
    /// reads as "it checked". A controller waits it on its injected clock.
    public static let leastWork = Duration(seconds: 0.6)
    /// 0.7: how long a done alert stays, its check and Recovered showing, before it closes.
    public static let doneHold = Duration(seconds: 0.7)
    /// 0.8: one breath of a status dot.
    static let pulse = Duration(seconds: 0.8)
    /// 0.9: one turn of the spinner.
    static let spin = Duration(seconds: 0.9)
    /// 1.6: one breath of the live badge.
    public static let breathe = Duration(seconds: 1.6)
    /// 2.4: how long a toast stays.
    public static let toast = Duration(seconds: 2.4)
    /// 2.4: one turn of the ring tracing round Atlas's mark while the viewer connects.
    static let trace = Duration(seconds: 2.4)
  }

  /// The springs behind the animations, kept as values so their duration and bounce can be read back. A spring
  /// bounces only where something earned it: a mark popping in once, and a success tile
  /// settling. Nothing the person did not throw overshoots.
  enum Springs {
    /// A button's spring and an alert settling: 0.3 seconds, no bounce.
    static let release = Spring(duration: 0.3, bounce: 0)
    /// A tint, a rim, a colour or a row moving: 0.3 seconds, no bounce.
    static let state = Spring(duration: 0.3, bounce: 0)
    /// A chip, a row, a slot, a toast or an alert appearing: 0.25 seconds, no bounce.
    static let arrive = Spring(duration: 0.25, bounce: 0)
    /// A check, a badge or a symbol popping in: 0.35 seconds, bounce 0.15.
    static let pop = Spring(duration: 0.35, bounce: 0.15)
    /// A success tile settling into place: 0.5 seconds, bounce 0.2.
    static let settle = Spring(duration: 0.5, bounce: 0.2)
    /// A tab's thumb gliding to the tab chosen: 0.25 seconds, no bounce.
    static let tab = Spring(duration: 0.25, bounce: 0)
    /// A side column sliding in: 0.35 seconds, no bounce (``Motion/slide``).
    static let slide = Spring(duration: 0.35, bounce: 0)
    /// A window or a pane changing height: 0.25 seconds, no bounce.
    static let resize = Spring(duration: 0.25, bounce: 0)
    /// A screenshot zooming, or the screen turning into the small screen and back: 0.35 seconds, no bounce, as Quick
    /// Look's zoom lands.
    static let zoom = Spring(duration: 0.35, bounce: 0)
    /// A position indicator moving, such as the page dots: 0.3 seconds, no bounce.
    static let indicator = Spring(duration: 0.3, bounce: 0)
  }

  /// The setup window's glide curve: it starts at once and lands softly, as a cubic Bézier's two control
  /// points, for `CAMediaTimingFunction(controlPoints:)`.
  public static let glideSoftCurve = (x1: Float(0.25), y1: Float(1), x2: Float(0.5), y2: Float(1))

  /// 0.8: how far through a glide the window's content starts fading back in, over ``Duration/fade``.
  public static let glideReveal = 0.8

  /// A window's resize curve: it leaves its start gently and settles softly, as a cubic Bézier's two control
  /// points, for `CAMediaTimingFunction(controlPoints:)`.
  public static let resizeCurve = (x1: Float(0.25), y1: Float(0.1), x2: Float(0.25), y2: Float(1))

  /// A text link's press: ease out over ``Duration/press``.
  static var press: Animation { .easeOut(duration: Duration.press.seconds) }

  /// A button's pressed fill easing back once it is let go: ease out over ``Duration/pressRelease``.
  static var pressRelease: Animation { .easeOut(duration: Duration.pressRelease.seconds) }

  /// Something coming back to rest with no bounce: ``Springs/release``.
  static var release: Animation { .spring(Springs.release) }

  /// A tint, a rim, a colour or a row moving to its new state: ``Springs/state``.
  static var state: Animation { .spring(Springs.state) }

  /// A chip, a row, a slot or a toast appearing: ``Springs/arrive``.
  static var arrive: Animation { .spring(Springs.arrive) }

  /// The same leaving: ease in over ``Duration/leave``.
  static var leave: Animation { .easeIn(duration: Duration.leave.seconds) }

  /// A check, a badge or a symbol popping in: ``Springs/pop``.
  static var pop: Animation { .spring(Springs.pop) }

  /// A success tile settling from 92% into place: ``Springs/settle``.
  static var settle: Animation { .spring(Springs.settle) }

  /// A step's number shrinking away as the step finishes: ease out over ``Duration/step``. It scales, so
  /// under Reduce Motion a view passes it through ``animation(_:reduceMotion:reduced:)``.
  static var stepOut: Animation { .easeOut(duration: Duration.step.seconds) }

  /// A step lighting up as the one before it finishes: ``state``, after ``Duration/step``.
  static var stepLight: Animation {
    state.delay(Duration.step.seconds)
  }

  /// A tab's thumb gliding to the tab chosen: ``Springs/tab``.
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

  /// Something waiting to show, such as a spinner: a fade over ``Duration/fade`` after
  /// ``Duration/appearDelay``, so a quick answer never flashes it. Only opacity changes, so Reduce Motion
  /// keeps it.
  static var appear: Animation {
    .easeOut(duration: Duration.fade.seconds)
      .delay(Duration.appearDelay.seconds)
  }

  /// A working note showing: a fade over ``Duration/fade`` after ``Duration/noteDelay``, so work that ends sooner
  /// never flashes it. Only opacity changes, so Reduce Motion keeps it.
  static var noteAppear: Animation {
    .easeOut(duration: Duration.fade.seconds)
      .delay(Duration.noteDelay.seconds)
  }

  /// An alert or a caption leaving: ease in over ``Duration/dismiss``. An alert shrinks as it fades, so under
  /// Reduce Motion it becomes ``reduced``.
  static var dismiss: Animation { .easeIn(duration: Duration.dismiss.seconds) }

  /// A pane leaving as another takes its place, such as the small screen's: ease in over
  /// ``Duration/crossfadeOut``.
  static var crossfadeOut: Animation { .easeIn(duration: Duration.crossfadeOut.seconds) }

  /// A tab's pane arriving over the last: ease out over ``Duration/swapIn``, at once.
  static var swapIn: Animation { .easeOut(duration: Duration.swapIn.seconds) }

  /// A tab's pane leaving as another arrives: ease in over ``Duration/swapOut``.
  static var swapOut: Animation { .easeIn(duration: Duration.swapOut.seconds) }

  /// A setup page arriving over the last: ease out over ``Duration/pageIn``, at once.
  static var pageIn: Animation { .easeOut(duration: Duration.pageIn.seconds) }

  /// A setup page leaving as the next arrives: ease in over ``Duration/pageOut``.
  static var pageOut: Animation { .easeIn(duration: Duration.pageOut.seconds) }

  /// A choice's mark filling or emptying, such as a radio's: ease out over ``Duration/hoverIn``. Only colour and
  /// opacity change, so Reduce Motion keeps it.
  static var markFill: Animation { .easeOut(duration: Duration.hoverIn.seconds) }

  /// Changing words swapping in place: ease out over ``Duration/textSwap``. Only opacity changes, so
  /// Reduce Motion keeps it.
  static var textSwap: Animation { .easeOut(duration: Duration.textSwap.seconds) }

  /// A pane's crossfade under Reduce Motion, both ways: ease out over ``Duration/crossfadeReduced``.
  static var crossfadeReduced: Animation { .easeOut(duration: Duration.crossfadeReduced.seconds) }

  /// A fade: ease out over ``Duration/fade``.
  static var fade: Animation { .easeOut(duration: Duration.fade.seconds) }

  /// A dot breathing in and out for as long as it shows.
  static var pulse: Animation {
    .easeInOut(duration: Duration.pulse.seconds)
      .repeatForever(autoreverses: true)
  }

  /// A mark breathing in and out for as long as it shows, one breath in and out over ``Duration/breathe``,
  /// each way half of it. A loop, so only where ``loops(reduceMotion:)`` allows.
  static var breathe: Animation {
    .easeInOut(duration: Duration.breathe.seconds / 2)
      .repeatForever(autoreverses: true)
  }

  /// A ring tracing round a mark for as long as it shows, one linear turn over ``Duration/trace``. A loop, so
  /// only where ``loops(reduceMotion:)`` allows.
  static var trace: Animation {
    .linear(duration: Duration.trace.seconds)
      .repeatForever(autoreverses: false)
  }

  /// The spinner turning for as long as it shows.
  static var spin: Animation {
    .linear(duration: Duration.spin.seconds)
      .repeatForever(autoreverses: false)
  }

  /// A side column sliding in, such as the viewer's Details: ``Springs/slide``, a spring with no bounce that
  /// turns back from where it is when asked to reverse mid-way. The same curve as the response-0.35,
  /// damping-1 spring it replaced.
  static var slide: Animation { .spring(Springs.slide) }

  /// A position indicator moving, such as the page dots: ``Springs/indicator``, with no bounce.
  static var indicator: Animation { .spring(Springs.indicator) }

  /// The shared fade for transitions under Reduce Motion: ease out over ``Duration/reduced``, with no
  /// movement. Callers can choose a fallback through ``animation(_:reduceMotion:)`` or
  /// ``animation(_:reduceMotion:reduced:)``.
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

  /// How changing content redraws: a morph, such as digits counting or words interpolating, or a crossfade under
  /// Reduce Motion, so nothing slides or reshapes.
  ///
  /// - Parameters:
  ///   - moving: The content transition, such as `.numericText()`.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: `moving`, or `.opacity` under Reduce Motion.
  static func contentTransition(_ moving: ContentTransition, reduceMotion: Bool) -> ContentTransition {
    reduceMotion ? .opacity : moving
  }

  /// Whether a loop runs, such as a breath or a spinning ring: never under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: False under Reduce Motion.
  public static func loops(reduceMotion: Bool) -> Bool {
    !reduceMotion
  }
}
