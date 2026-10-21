import AppKit
import AtlasKit
import AtlasUI

/// The menu bar glyph: Atlas's orbit, a flat planet with a ring passing in
/// front of it. Control turns it into a solid tile with the orbit cut out,
/// and an unavailable helper dims it. Work and trouble add a coloured dot
/// beside it, as the system's own indicators do, so only those two are not
/// templates; they draw in the menu bar's label colour instead.
///
/// Its numbers (`Layout`) are the glyph's drawing geometry, sized to the
/// menu bar, not design tokens, so they stay here as they are.
enum StatusGlyph {
  /// The glyph for a mode.
  ///
  /// - Parameter mode: The helper's mode, as the person reads it.
  /// - Returns: The image for the status item's button, a template unless it carries a coloured dot.
  static func image(for mode: Mode) -> NSImage {
    let dot: NSColor? =
      switch mode.mark {
      case .working: .systemBlue
      case .alert, .dash, .stopped: .systemOrange
      default: nil
      }
    let size = NSSize(width: dot == nil ? Layout.box.width : Layout.boxWithDot, height: Layout.box.height)
    let image = NSImage(size: size, flipped: true) { _ in
      let pixel = 1 / Self.pixelScale()
      // Drawn opaque in one layer, then faded as a whole, so overlapping
      // strokes never darken: by the label colour's own alpha, and more
      // when dimmed.
      let ink = dot == nil ? NSColor.black : (NSColor.labelColor.usingColorSpace(.sRGB) ?? .black)
      ink
        .withAlphaComponent(1)
        .set()
      guard let context = NSGraphicsContext.current?.cgContext else { return false }
      context.saveGState()
      context.setAlpha(ink.alphaComponent * (mode.mark == .dimmed ? Layout.dimmed : 1))
      context.beginTransparencyLayer(auxiliaryInfo: nil)
      // The ring a little bolder than the drawing's, so it and the gap
      // beside it stay clear on Retina displays.
      let center = CGPoint(x: Layout.box.width / 2, y: Layout.box.height / 2)
      if mode.mark == .filled {
        NSBezierPath(
          roundedRect: NSRect(origin: .zero, size: Layout.box),
          xRadius: Layout.tileCorner,
          yRadius: Layout.tileCorner
        )
        .fill()
        Orbit(center: center, width: Layout.tileOrbit, pixel: pixel, least: Layout.tileOrbitLeast)
          .draw(in: context, cutOut: true)
      } else {
        Orbit(center: center, width: Layout.orbit, pixel: pixel, least: Layout.orbitLeast)
          .draw(in: context, cutOut: false)
      }
      context.endTransparencyLayer()
      context.restoreGState()
      if let dot {
        dot.setFill()
        // Stopped draws a square, a stop button's shape, so it reads apart from the alert's round dot.
        let mark =
          mode.mark == .stopped
          ? NSBezierPath(roundedRect: Layout.dot, xRadius: Layout.stopCorner, yRadius: Layout.stopCorner)
          : NSBezierPath(ovalIn: Layout.dot)
        mark.fill()
      }
      return true
    }
    image.isTemplate = dot == nil
    image.accessibilityDescription = PanelWords.statusItem(mode.label)
    return image
  }

  /// Device pixels per point of the context being drawn into, so lines stay
  /// crisp on both standard and Retina displays.
  ///
  /// - Returns: `Layout.fallbackScale` when no context is current.
  private static func pixelScale() -> CGFloat {
    guard let transform = NSGraphicsContext.current?.cgContext.userSpaceToDeviceSpaceTransform else {
      return Layout.fallbackScale
    }
    let scale = (transform.a * transform.a + transform.b * transform.b)
      .squareRoot()
      .rounded()
    return max(1, scale)
  }
}

/// The glyph's drawing geometry, in points of the menu bar.
private enum Layout {
  /// 18 by 16: the orbit's box, and the solid tile in control.
  static let box = NSSize(width: 18, height: 16)
  /// 25: the box's width with a dot beside the orbit.
  static let boxWithDot: CGFloat = 25
  /// 4: the solid tile's corners.
  static let tileCorner: CGFloat = 4
  /// 17.4: the orbit's width, the ring's outer reach.
  static let orbit: CGFloat = 17.4
  /// 1: the thinnest the orbit's line draws.
  static let orbitLeast: CGFloat = 1
  /// 14: the orbit's width cut out of the solid tile.
  static let tileOrbit: CGFloat = 14
  /// 0.8: the thinnest the cut-out orbit's line draws.
  static let tileOrbitLeast: CGFloat = 0.8
  /// The dot beside the orbit: 4.5 across, at 19.25, centred on the box's height.
  static let dot = NSRect(x: 19.25, y: 5.75, width: 4.5, height: 4.5)
  /// 1: the stopped mark's corner radius, a square softened as the system's stop buttons are.
  static let stopCorner: CGFloat = 1
  /// 0.4: how strongly the glyph draws while the helper cannot be reached.
  static let dimmed: CGFloat = 0.4
  /// 2: the pixels a point is taken to have when no context is current, a Retina display's.
  static let fallbackScale: CGFloat = 2
}
