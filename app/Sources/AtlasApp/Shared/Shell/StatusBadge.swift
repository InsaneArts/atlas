import AppKit
import AtlasUI

/// The menu bar item's badge: a shape layer over the status button's image, in the hole ``StatusGlyph`` knocks out of
/// the orbit for it. Its shape and colour come from the look; while the look breathes, a Core Animation repeat fades it
/// 100 → 50% and back over ``Motion/Duration/breathe`` (v3 M3), never a timer, and under Reduce Motion it holds
/// still.
final class StatusBadge {
  /// The breath's key on the layer.
  static let breathKey = "breath"

  /// The layer, added to the status button's own layer.
  let layer = CAShapeLayer()

  /// A hidden badge. Its path, colour, frame and visibility change at once, with no implicit animation.
  init() {
    layer.isHidden = true
    layer.zPosition = Layout.above
    layer.actions = Self.still
  }

  /// Shows the badge a look asks for, or hides it.
  ///
  /// - Parameters:
  ///   - look: The item's look.
  ///   - image: Where the button draws its image, the item's square, in the host's coordinates.
  ///   - flipped: Whether the host runs top down.
  ///   - appearance: The menu bar's appearance, which settles the system colour.
  ///   - reduceMotion: Whether Reduce Motion is on.
  func show(
    _ look: StatusGlyph.Look,
    in image: CGRect,
    flipped: Bool,
    appearance: NSAppearance,
    reduceMotion: Bool
  ) {
    guard let box = StatusGlyph.frame(of: look.badge), let colour = StatusGlyph.colour(of: look.badge) else {
      layer.isHidden = true
      layer.removeAnimation(forKey: Self.breathKey)
      return
    }
    layer.frame = Self.place(box, in: image, flipped: flipped)
    layer.path = StatusGlyph.outline(of: look.badge, in: CGRect(origin: .zero, size: box.size))
    var fill: CGColor?
    appearance.performAsCurrentDrawingAppearance { fill = colour.cgColor }
    layer.fillColor = fill
    layer.isHidden = false
    if look.breathes, Motion.loops(reduceMotion: reduceMotion) {
      if layer.animation(forKey: Self.breathKey) == nil {
        layer.add(Self.breath(), forKey: Self.breathKey)
      }
    } else {
      layer.removeAnimation(forKey: Self.breathKey)
    }
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

  /// No implicit animation on the properties a state change sets, so a new state shows at once.
  private static let still: [String: any CAAction] = [
    "path": NSNull(),
    "fillColor": NSNull(),
    "hidden": NSNull(),
    "position": NSNull(),
    "bounds": NSNull(),
  ]
}

/// The badge's own measures.
private enum Layout {
  /// 0.5: the badge's opacity at the bottom of a breath (v3 M3).
  static let breathLow = 0.5
  /// 1: above anything the button draws in its own layer.
  static let above: CGFloat = 1
}
