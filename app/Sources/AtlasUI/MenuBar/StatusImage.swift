import AppKit
import AtlasKit

/// The menu bar glyph: Atlas's orbit, a flat planet with a ring passing in
/// front of it. Control turns it into a solid tile with the orbit cut out,
/// and an unavailable helper dims it. Work and trouble add a coloured dot
/// beside it, as the system's own indicators do, so only those two are not
/// templates; they draw in the menu bar's label colour instead.
func statusImage(_ mode: Mode) -> NSImage {
    let dot: NSColor? =
        switch mode.mark {
        case .working: .systemBlue
        case .alert, .dash: .systemOrange
        default: nil
        }
    let image = NSImage(size: NSSize(width: dot == nil ? 18 : 25, height: 16), flipped: true) { _ in
        let pixel = 1 / pixelScale()
        // Drawn opaque in one layer, then faded as a whole, so overlapping
        // strokes never darken: by the label colour's own alpha, and more
        // when dimmed.
        let ink = dot == nil ? NSColor.black : (NSColor.labelColor.usingColorSpace(.sRGB) ?? .black)
        ink.withAlphaComponent(1).set()
        guard let context = NSGraphicsContext.current?.cgContext else { return false }
        context.saveGState()
        context.setAlpha(ink.alphaComponent * (mode.mark == .dimmed ? 0.4 : 1))
        context.beginTransparencyLayer(auxiliaryInfo: nil)
        // The ring a little bolder than the drawing's, so it and the gap
        // beside it stay clear on Retina displays.
        let center = CGPoint(x: 9, y: 8)
        if mode.mark == .filled {
            NSBezierPath(
                roundedRect: NSRect(x: 0, y: 0, width: 18, height: 16), xRadius: 4, yRadius: 4
            ).fill()
            Orbit(center: center, width: 14, pixel: pixel, least: 0.8).draw(in: context, cutOut: true)
        } else {
            Orbit(center: center, width: 17.4, pixel: pixel, least: 1).draw(in: context, cutOut: false)
        }
        context.endTransparencyLayer()
        context.restoreGState()
        if let dot {
            dot.setFill()
            NSBezierPath(ovalIn: NSRect(x: 19.25, y: 5.75, width: 4.5, height: 4.5)).fill()
        }
        return true
    }
    image.isTemplate = dot == nil
    image.accessibilityDescription = "Atlas: \(mode.label)"
    return image
}

/// The orbit in a flipped (top-down) box, from its 64-unit drawing: a flat
/// sphere and a ring tilted 20 degrees, right side up, whose front half
/// passes over the sphere with a clear gap on either side. Only the sphere
/// is cut for the gap; the ring is whole. OrbitMark and the app icon
/// (app/Resources/AppIcon.swift) draw the same shape: change them together.
struct Orbit {
    /// The ring's line, in the box's points.
    let line: CGFloat
    let sphere: CGPath
    /// The ring's centre line, all the way round.
    let ring: CGPath
    /// The lower half of the ring, which passes in front of the sphere.
    let front: CGPath

    /// The orbit `width` points wide (the ring's outer reach) around
    /// `center`. Its line keeps to the drawing's 2.4 units but never thins
    /// below 1.2 device pixels of `pixel` points each, so it holds on
    /// standard displays, or below `least` points.
    init(center: CGPoint, width: CGFloat, pixel: CGFloat, least: CGFloat = 0) {
        let scale = width / 50.8
        line = max(2.4 * scale, 1.2 * pixel, least)
        var place = CGAffineTransform(translationX: center.x, y: center.y).scaledBy(x: scale, y: scale)
        sphere = CGPath(ellipseIn: CGRect(x: -14.6, y: -14.6, width: 29.2, height: 29.2), transform: &place)
        var tilt = place.rotated(by: -20 * .pi / 180).scaledBy(x: 25.6, y: 6.9)
        ring = CGPath(ellipseIn: CGRect(x: -1, y: -1, width: 2, height: 2), transform: &tilt)
        let front = CGMutablePath()
        front.addArc(center: .zero, radius: 1, startAngle: 0, endAngle: .pi, clockwise: false, transform: tilt)
        self.front = front
    }

    /// The gap's width across the front of the ring, 0.6 of a line clear
    /// on each side of it.
    var gap: CGFloat { 2.2 * line }

    /// Draw in the current colour. `cutOut` clears the shape from what is
    /// already drawn instead, for the solid tile.
    func draw(in context: CGContext, cutOut: Bool) {
        context.saveGState()
        context.setBlendMode(cutOut ? .destinationOut : .normal)
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

/// Device pixels per point of the context being drawn into, so lines stay
/// crisp on both standard and Retina displays.
private func pixelScale() -> CGFloat {
    guard let t = NSGraphicsContext.current?.cgContext.userSpaceToDeviceSpaceTransform else {
        return 2
    }
    return max(1, (t.a * t.a + t.b * t.b).squareRoot().rounded())
}
