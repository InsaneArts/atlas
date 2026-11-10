import AppKit
import AtlasUI

extension StatusGlyph {
  /// Draws the Screen glyph in the current colour, top down: the display's outline, its stand, and the orbit on the
  /// display. Every line keeps at least 1.2 device pixels, as the orbit's does, so it holds on standard displays.
  ///
  /// - Parameters:
  ///   - context: The context to draw into, already in the glyph's one layer.
  ///   - pixel: One device pixel, in points.
  static func drawScreen(in context: CGContext, pixel: CGFloat) {
    let least = 1.2 * pixel
    context.saveGState()
    context.setLineWidth(max(StatusScreenLayout.stroke, least))
    context.addPath(
      CGPath(
        roundedRect: StatusScreenLayout.display,
        cornerWidth: StatusScreenLayout.corner,
        cornerHeight: StatusScreenLayout.corner,
        transform: nil
      )
    )
    context.strokePath()
    context.setLineWidth(max(StatusScreenLayout.standStroke, least))
    context.setLineCap(.round)
    context.move(to: StatusScreenLayout.stand.from)
    context.addLine(to: StatusScreenLayout.stand.to)
    context.strokePath()
    context.restoreGState()
    Orbit(
      center: StatusScreenLayout.orbitCentre,
      width: StatusScreenLayout.orbit,
      pixel: pixel,
      least: StatusScreenLayout.orbitLeast
    )
    .draw(in: context)
  }
}
