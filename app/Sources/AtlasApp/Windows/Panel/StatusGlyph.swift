import AppKit
import AtlasKit
import AtlasUI

/// The menu bar glyph: Atlas's orbit, a flat planet with a ring passing in
/// front of it. Control turns it into a solid tile with the orbit cut out,
/// and an unavailable helper dims it. Work and trouble add a coloured dot
/// beside it, as the system's own indicators do, so only those two are not
/// templates; they draw in the menu bar's label colour instead.
///
/// Its numbers (the 18 by 16 box, the tile's 4-point corners, the orbit's
/// 17.4 and 14 widths, the dot at 19.25 and 4.5 across, the 0.4 dimming)
/// are the glyph's drawing geometry, sized to the menu bar, not design
/// tokens, so they stay here as they are.
enum StatusGlyph {
  /// The glyph for a mode.
  ///
  /// - Parameter mode: The helper's mode, as the person reads it.
  /// - Returns: The image for the status item's button, a template unless it carries a coloured dot.
  static func image(for mode: Mode) -> NSImage {
    let dot: NSColor? =
      switch mode.mark {
      case .working: .systemBlue
      case .alert, .dash: .systemOrange
      default: nil
      }
    let image = NSImage(size: NSSize(width: dot == nil ? 18 : 25, height: 16), flipped: true) { _ in
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
      context.setAlpha(ink.alphaComponent * (mode.mark == .dimmed ? 0.4 : 1))
      context.beginTransparencyLayer(auxiliaryInfo: nil)
      // The ring a little bolder than the drawing's, so it and the gap
      // beside it stay clear on Retina displays.
      let center = CGPoint(x: 9, y: 8)
      if mode.mark == .filled {
        NSBezierPath(
          roundedRect: NSRect(x: 0, y: 0, width: 18, height: 16),
          xRadius: 4,
          yRadius: 4
        )
        .fill()
        Orbit(center: center, width: 14, pixel: pixel, least: 0.8)
          .draw(in: context, cutOut: true)
      } else {
        Orbit(center: center, width: 17.4, pixel: pixel, least: 1)
          .draw(in: context, cutOut: false)
      }
      context.endTransparencyLayer()
      context.restoreGState()
      if let dot {
        dot.setFill()
        NSBezierPath(ovalIn: NSRect(x: 19.25, y: 5.75, width: 4.5, height: 4.5))
          .fill()
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
  /// - Returns: 2 when no context is current.
  private static func pixelScale() -> CGFloat {
    guard let t = NSGraphicsContext.current?.cgContext.userSpaceToDeviceSpaceTransform else {
      return 2
    }
    let scale = (t.a * t.a + t.b * t.b)
      .squareRoot()
      .rounded()
    return max(1, scale)
  }
}
