import AppKit
import AtlasUI

/// The menu bar item's badge: a shape layer over the status button's image, in the hole ``StatusGlyph`` knocks out of
/// the orbit for it. Its shape and colour come from the look.
///
/// A disc or a square is filled; a ring is stroked, its fill clear, so the menu bar shows through its middle.
///
/// A state change moves it: a badge arriving scales in from nothing on ``Motion/LayerSpring/pop``, and one badge giving
/// way to another crossfades its fill, its stroke and its shape, disc to square or ring and back, over
/// ``Motion/Duration/crossfade``, letting a pop that runs finish. A badge leaving goes at once, with the ring the
/// orbit closes around it. While the look breathes, a Core Animation repeat fades it 100 → 50% and back over
/// ``Motion/Duration/breathe``, never a timer; the breath moves its opacity and the pop its scale, so neither
/// stops the other, and when the breath stops its opacity settles back to whole over the crossfade. Under Reduce
/// Motion a new state swaps in at once and nothing breathes; while a menu is open a new state swaps in at once, a
/// breath that runs keeps running, and none starts. The model values are always the new state's, set at once, so a
/// removed animation leaves the badge final.
final class StatusBadge {
  /// The breath's key on the layer.
  static let breathKey = "breath"
  /// The pop's key on the layer.
  static let popKey = "pop"
  /// The colour crossfade's key on the layer.
  static let colourKey = "colour"
  /// The shape crossfade's key on the layer.
  static let shapeKey = "shape"
  /// The stroke's colour crossfade's key on the layer.
  static let strokeKey = "stroke"
  /// The key of the opacity settling to whole when the breath stops.
  static let settleKey = "settle"
  /// The keys of every animation a state change starts, as opposed to the breath.
  static let changeKeys = [popKey, colourKey, shapeKey, strokeKey, settleKey]

  /// The layer, added to the status button's own layer.
  let layer = CAShapeLayer()
  /// Called once the animations of a state change have finished, so the item can draw its final state again.
  var finished: (@MainActor () -> Void)?
  /// The badge shown now; none while hidden.
  private var shown = StatusGlyph.Badge.none

  /// A hidden badge. Its path, colour, frame and visibility change only as ``show(_:in:flipped:appearance:motion:)``
  /// animates them, with no implicit animation.
  init() {
    layer.isHidden = true
    layer.zPosition = Layout.above
    layer.actions = Self.still
    layer.lineWidth = StatusGlyph.ringStroke
  }

  /// Shows the badge a look asks for, or hides it.
  ///
  /// - Parameters:
  ///   - look: The item's look.
  ///   - image: Where the button draws its image, the item's square, in the host's coordinates.
  ///   - flipped: Whether the host runs top down.
  ///   - appearance: The menu bar's appearance, which settles the system colour.
  ///   - motion: What may move: everything, only a breath that runs while a menu is open, or nothing under Reduce
  ///     Motion.
  func show(
    _ look: StatusGlyph.Look,
    in image: CGRect,
    flipped: Bool,
    appearance: NSAppearance,
    motion: StatusMotion
  ) {
    let before = shown
    shown = look.badge
    guard let box = StatusGlyph.frame(of: look.badge), let colour = StatusGlyph.colour(of: look.badge) else {
      layer.isHidden = true
      layer.removeAllAnimations()
      return
    }
    // What shows now, read before any animation is removed.
    let now = layer.presentation() ?? layer
    layer.frame = Self.place(box, in: image, flipped: flipped)
    let path = StatusGlyph.outline(of: look.badge, in: CGRect(origin: .zero, size: box.size))
    var ink: CGColor?
    appearance.performAsCurrentDrawingAppearance { ink = colour.cgColor }
    let clear = NSColor.clear.cgColor
    let hollow = StatusGlyph.isHollow(look.badge)
    let paint = Paint(fill: hollow ? clear : ink, stroke: hollow ? ink : clear)
    if before != look.badge {
      change(from: before, now: now, path: path, paint: paint, moves: motion == .all)
    }
    layer.path = path
    layer.fillColor = paint.fill
    layer.strokeColor = paint.stroke
    layer.isHidden = false
    breathe(look.breathes, now: now, motion: motion)
  }

  /// Stops every animation a state change started, leaving the badge as its model values say, final; the breath
  /// keeps running. For a menu opening, so nothing moves in the frame its tracking starts.
  func settle() {
    for key in Self.changeKeys {
      layer.removeAnimation(forKey: key)
    }
  }

  /// Moves the badge from the one shown before to a new one, about to be set: a pop when none showed, a crossfade
  /// of fill, stroke and shape from what shows now when one did, letting a pop that runs finish, or, when nothing
  /// may move, no animation at all, every one still running from an earlier change stopped.
  ///
  /// - Parameters:
  ///   - before: The badge shown before.
  ///   - now: The layer as it shows now.
  ///   - path: The new badge's outline.
  ///   - paint: The new badge's fill and stroke.
  ///   - moves: Whether the change may animate.
  private func change(from before: StatusGlyph.Badge, now: CAShapeLayer, path: CGPath, paint: Paint, moves: Bool) {
    guard moves else {
      settle()
      return
    }
    for key in [Self.colourKey, Self.shapeKey, Self.strokeKey, Self.settleKey] {
      layer.removeAnimation(forKey: key)
    }
    guard before != .none else {
      run(Self.pop(), forKey: Self.popKey)
      return
    }
    if now.fillColor != paint.fill {
      run(Self.crossfade("fillColor", from: now.fillColor, to: paint.fill), forKey: Self.colourKey)
    }
    if now.strokeColor != paint.stroke {
      run(Self.crossfade("strokeColor", from: now.strokeColor, to: paint.stroke), forKey: Self.strokeKey)
    }
    if now.path != path {
      run(Self.crossfade("path", from: now.path, to: path), forKey: Self.shapeKey)
    }
  }

  /// Starts or stops the breath: it starts only when everything may move, keeps running while a menu is open, and
  /// stops under Reduce Motion or when the look no longer breathes. Stopped while everything may move, the badge's
  /// opacity settles from where the breath left it back to whole.
  ///
  /// - Parameters:
  ///   - breathes: Whether the look breathes.
  ///   - now: The layer as it showed before this change.
  ///   - motion: What may move.
  private func breathe(_ breathes: Bool, now: CAShapeLayer, motion: StatusMotion) {
    let breathing = layer.animation(forKey: Self.breathKey) != nil
    if breathes, motion != .still {
      if !breathing, motion == .all {
        layer.add(Self.breath(), forKey: Self.breathKey)
      }
      return
    }
    guard breathing else { return }
    layer.removeAnimation(forKey: Self.breathKey)
    guard motion == .all else { return }
    run(Self.crossfade("opacity", from: now.opacity, to: Float(Layout.whole)), forKey: Self.settleKey)
  }

  /// Adds a state change's animation in a transaction of its own, which calls ``finished`` when it ends.
  ///
  /// - Parameters:
  ///   - animation: The animation.
  ///   - key: Its key.
  private func run(_ animation: CAAnimation, forKey key: String) {
    CATransaction.begin()
    CATransaction.setCompletionBlock { [weak self] in
      MainActor.assumeIsolated { self?.finished?() }
    }
    layer.add(animation, forKey: key)
    CATransaction.commit()
  }

  /// The pop: the badge's scale from nothing to whole on ``Motion/LayerSpring/pop``, running until the spring
  /// settles, so its overshoot plays out.
  ///
  /// - Returns: The animation.
  static func pop() -> CASpringAnimation {
    let spring = Motion.LayerSpring.pop
    let pop = CASpringAnimation(keyPath: "transform.scale")
    pop.mass = spring.mass
    pop.stiffness = spring.stiffness
    pop.damping = spring.damping
    pop.fromValue = Layout.nothing
    pop.toValue = Layout.whole
    pop.duration = pop.settlingDuration
    return pop
  }

  /// A crossfade of one of the badge's properties, easing out over ``Motion/Duration/crossfade``.
  ///
  /// - Parameters:
  ///   - keyPath: The property: its fill, its stroke or its outline.
  ///   - from: What it shows now.
  ///   - to: What it shows next.
  /// - Returns: The animation.
  static func crossfade(_ keyPath: String, from: Any?, to: Any?) -> CABasicAnimation {
    let crossfade = CABasicAnimation(keyPath: keyPath)
    crossfade.fromValue = from
    crossfade.toValue = to
    crossfade.duration = Motion.Duration.crossfade.seconds
    crossfade.timingFunction = CAMediaTimingFunction(name: .easeOut)
    return crossfade
  }

  /// The breath: opacity 1 → 0.5, easing in and out, reversed and repeated for as long as the layer shows, so one
  /// breath in and out takes ``Motion/Duration/breathe``.
  ///
  /// - Returns: The animation.
  static func breath() -> CABasicAnimation {
    let breath = CABasicAnimation(keyPath: "opacity")
    breath.fromValue = 1
    breath.toValue = Layout.breathLow
    breath.duration = Motion.Duration.breathe.seconds / 2
    breath.autoreverses = true
    breath.repeatCount = .infinity
    breath.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
    breath.isRemovedOnCompletion = false
    return breath
  }

  /// A badge's box in the host: the box in the item's square, moved to where the image is drawn, counted from the
  /// image's bottom edge when the host runs bottom up.
  ///
  /// - Parameters:
  ///   - box: The badge's box in the item's square, top down.
  ///   - image: Where the image is drawn in the host.
  ///   - flipped: Whether the host runs top down.
  /// - Returns: The layer's frame.
  static func place(_ box: CGRect, in image: CGRect, flipped: Bool) -> CGRect {
    let y = flipped ? image.minY + box.minY : image.maxY - box.maxY
    return CGRect(x: image.minX + box.minX, y: y, width: box.width, height: box.height)
  }

  /// No implicit animation on the properties a state change sets; only the explicit ones above move them.
  private static let still: [String: any CAAction] = [
    "path": NSNull(),
    "fillColor": NSNull(),
    "strokeColor": NSNull(),
    "hidden": NSNull(),
    "position": NSNull(),
    "bounds": NSNull(),
  ]
}

/// The badge's own measures.
private enum Layout {
  /// 0.5: the badge's opacity at the bottom of a breath.
  static let breathLow = 0.5
  /// 1: above anything the button draws in its own layer.
  static let above: CGFloat = 1
  /// 0: the badge's scale as a pop starts, nothing.
  static let nothing: CGFloat = 0
  /// 1: the badge whole, the scale a pop lands on and the opacity the breath settles back to.
  static let whole: CGFloat = 1
}
