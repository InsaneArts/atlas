import CoreGraphics

/// The orbit in a flipped (top-down) box, from its 64-unit drawing: a flat sphere and a ring tilted 20 degrees,
/// right side up, whose front half passes over the sphere with a clear gap on either side. Only the sphere is cut
/// for the gap; the ring is whole. `OrbitMark`, the menu bar glyph and the app icon (app/Resources/AppIcon.swift)
/// draw the same shape: change them together.
///
/// Its numbers (50.8, 14.6, 25.6, 6.9, 2.4…) are the drawing's own geometry, mirrored by the app icon, not design
/// tokens, so they stay here as they are.
public struct Orbit {
  /// The ring's line, in the box's points.
  let line: CGFloat
  /// The flat planet.
  let sphere: CGPath
  /// The ring's centre line, all the way round.
  let ring: CGPath
  /// The lower half of the ring, which passes in front of the sphere.
  let front: CGPath

  /// The orbit `width` points wide (the ring's outer reach) around `center`. Its line keeps to the drawing's
  /// 2.4 units but never thins below 1.2 device pixels of `pixel` points each, so it holds on standard displays,
  /// or below `least` points.
  ///
  /// - Parameters:
  ///   - center: The orbit's centre in the box.
  ///   - width: The ring's outer reach, in points.
  ///   - pixel: One device pixel, in points.
  ///   - least: The thinnest line allowed, in points.
  public init(center: CGPoint, width: CGFloat, pixel: CGFloat, least: CGFloat = 0) {
    let scale = width / 50.8
    line = max(2.4 * scale, 1.2 * pixel, least)
    var place = CGAffineTransform(translationX: center.x, y: center.y)
      .scaledBy(x: scale, y: scale)
    sphere = CGPath(ellipseIn: CGRect(x: -14.6, y: -14.6, width: 29.2, height: 29.2), transform: &place)
    var tilt =
      place
      .rotated(by: -20 * .pi / 180)
      .scaledBy(x: 25.6, y: 6.9)
    ring = CGPath(ellipseIn: CGRect(x: -1, y: -1, width: 2, height: 2), transform: &tilt)
    let front = CGMutablePath()
    front.addArc(center: .zero, radius: 1, startAngle: 0, endAngle: .pi, clockwise: false, transform: tilt)
    self.front = front
  }

  /// The gap's width across the front of the ring, 0.6 of a line clear on each side of it.
  var gap: CGFloat { 2.2 * line }

  /// Draws the orbit in the current colour.
  ///
  /// - Parameter context: The context to draw into.
  public func draw(in context: CGContext) {
    context.saveGState()
    context.beginTransparencyLayer(auxiliaryInfo: nil)
    context.setLineWidth(line)
    context.addPath(ring)
    context.strokePath()
    // The sphere in a layer of its own, so the gap clears only it and
    // the back of the ring stays hidden behind it.
    context.saveGState()
    context.beginTransparencyLayer(auxiliaryInfo: nil)
    context.addPath(sphere)
    context.fillPath()
    context.setBlendMode(.clear)
    context.setLineWidth(gap)
    context.addPath(front)
    context.strokePath()
    context.endTransparencyLayer()
    context.restoreGState()
    context.addPath(front)
    context.strokePath()
    context.endTransparencyLayer()
    context.restoreGState()
  }
}
