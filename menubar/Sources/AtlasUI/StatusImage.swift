import AppKit
import AtlasKit

/// The menu bar glyph: Atlas's orbit, a solid sphere with a ring passing in
/// front of it. Control turns it into a solid tile with the orbit cut out,
/// and an unavailable helper dims it. Work and trouble add a coloured dot
/// beside it, as the system's own indicators do, so only those two are not
/// templates; they draw in the menu bar's label colour instead.
public func statusImage(_ mode: Mode) -> NSImage {
    let dot: NSColor? =
        switch mode.mark {
        case .working: .systemBlue
        case .alert, .dash: .systemOrange
        default: nil
        }
    let image = NSImage(size: NSSize(width: dot == nil ? 18 : 25, height: 16), flipped: true) { _ in
        let line: CGFloat = pixelScale() < 1.5 ? 1.5 : 1.3
        // Drawn opaque in one layer, then faded as a whole, so overlapping
        // strokes never darken: by the label colour's own alpha, and more
        // when dimmed.
        let ink = dot == nil ? NSColor.black : (NSColor.labelColor.usingColorSpace(.sRGB) ?? .black)
        ink.withAlphaComponent(1).set()
        let context = NSGraphicsContext.current?.cgContext
        context?.saveGState()
        context?.setAlpha(ink.alphaComponent * (mode.mark == .dimmed ? 0.4 : 1))
        context?.beginTransparencyLayer(auxiliaryInfo: nil)
        if mode.mark == .filled {
            NSBezierPath(
                roundedRect: NSRect(x: 0, y: 0, width: 18, height: 16), xRadius: 4, yRadius: 4
            ).fill()
            Orbit(center: NSPoint(x: 9, y: 8), scale: 0.72).draw(line: line, cutOut: true)
        } else {
            Orbit(center: NSPoint(x: 9, y: 8), scale: 1).draw(line: line, cutOut: false)
        }
        context?.endTransparencyLayer()
        context?.restoreGState()
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

/// The orbit in a flipped (top-down) box: a sphere and a tilted ring whose
/// front half passes over it, kept apart from it by a clear gap above. The
/// app icon (menubar/Resources/AppIcon.swift) draws the same shape.
struct Orbit {
    let center: NSPoint
    let scale: CGFloat

    /// Draw in the current colour. `cutOut` clears the shape from what is
    /// already drawn instead, for the solid tile.
    func draw(line: CGFloat, cutOut: Bool) {
        let context = NSGraphicsContext.current
        let erase: NSCompositingOperation = cutOut ? .clear : .sourceOver
        let restore: NSCompositingOperation = cutOut ? .sourceOver : .clear
        let radius = 4.6 * scale
        let sphere = NSBezierPath(
            ovalIn: NSRect(
                x: center.x - radius, y: center.y - radius, width: 2 * radius, height: 2 * radius))
        context?.compositingOperation = erase
        stroke(ring(from: 0, to: 360), width: line)
        sphere.fill()
        // A gap where the ring's front crosses the sphere, then the sphere
        // back below the ring, so the gap shows only above it.
        context?.compositingOperation = restore
        stroke(ring(from: 40, to: 140), width: line + 1.3 * scale)
        context?.compositingOperation = erase
        NSGraphicsContext.saveGraphicsState()
        sphere.addClip()
        let below = NSBezierPath()
        below.appendArc(withCenter: .zero, radius: 1, startAngle: 0, endAngle: 180, clockwise: false)
        below.line(to: NSPoint(x: -1, y: 10))
        below.line(to: NSPoint(x: 1, y: 10))
        below.close()
        below.transform(using: tilt)
        below.fill()
        NSGraphicsContext.restoreGraphicsState()
        stroke(ring(from: 0, to: 180), width: line)
        context?.compositingOperation = .sourceOver
    }

    /// Unit-circle geometry to the ring's tilted ellipse.
    private var tilt: AffineTransform {
        let t = CGAffineTransform(translationX: center.x, y: center.y)
            .rotated(by: -20 * .pi / 180)
            .scaledBy(x: 8.2 * scale, y: 2.5 * scale)
        return AffineTransform(m11: t.a, m12: t.b, m21: t.c, m22: t.d, tX: t.tx, tY: t.ty)
    }

    /// Part of the ring, by angle; 0 to 180 degrees is the half in front.
    private func ring(from start: CGFloat, to end: CGFloat) -> NSBezierPath {
        let path = NSBezierPath()
        path.appendArc(
            withCenter: .zero, radius: 1, startAngle: start, endAngle: end, clockwise: false)
        path.transform(using: tilt)
        return path
    }

    private func stroke(_ path: NSBezierPath, width: CGFloat) {
        path.lineWidth = width
        path.lineCapStyle = .butt
        path.stroke()
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
