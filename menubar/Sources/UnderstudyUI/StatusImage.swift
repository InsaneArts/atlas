import AppKit
import UnderstudyKit

/// The menu bar glyph: a monochrome template "U" whose right arm ends in a
/// detached cap, the stand-in. Marks sit in the bowl; control turns it into
/// a solid tile with the letter cut out. The shape, not a colour, carries
/// the state, like the system's own items.
public func statusImage(_ mode: Mode) -> NSImage {
    let image = NSImage(size: NSSize(width: 15, height: 16), flipped: true) { _ in
        let scale = pixelScale()
        // Whole pixels on a standard display, a lighter stem on Retina.
        let stem: CGFloat = scale < 1.5 ? 2 : 1.5
        let ink = NSColor.black.withAlphaComponent(mode.mark == .dimmed ? 0.4 : 1)
        ink.set()
        guard mode.mark != .filled else {
            NSBezierPath(
                roundedRect: NSRect(x: 0, y: 0, width: 15, height: 16), xRadius: 4, yRadius: 4
            ).fill()
            NSGraphicsContext.current?.compositingOperation = .clear
            letterU(left: 4, right: 11, top: 3, bottom: 13, stem: stem, gapAt: 5).fill()
            NSGraphicsContext.current?.compositingOperation = .sourceOver
            return true
        }
        letterU(left: 2, right: 13, top: 1, bottom: 15, stem: stem, gapAt: 4).fill()
        switch mode.mark {
        case .working:
            ellipsis(center: NSPoint(x: 7.5, y: 8.5), scale: scale)
        case .alert:
            NSBezierPath(rect: NSRect(x: 7, y: 4, width: 1, height: 5)).fill()
            NSBezierPath(rect: NSRect(x: 7, y: 10, width: 1, height: 1)).fill()
        case .dash:
            NSBezierPath(rect: NSRect(x: 5, y: 8, width: 5, height: 1)).fill()
        default:
            break
        }
        return true
    }
    image.isTemplate = true
    image.accessibilityDescription = "Understudy: \(mode.label)"
    return image
}

/// A "U" with whole-pixel stems and a one-point break that detaches the top
/// of the right arm.
private func letterU(
    left: CGFloat, right: CGFloat, top: CGFloat, bottom: CGFloat, stem: CGFloat, gapAt: CGFloat
) -> NSBezierPath {
    let radius = (right - left) / 2
    let center = NSPoint(x: (left + right) / 2, y: bottom - radius)
    let path = NSBezierPath()
    path.move(to: NSPoint(x: left, y: top))
    path.line(to: NSPoint(x: left, y: center.y))
    path.appendArc(
        withCenter: center, radius: radius, startAngle: 180, endAngle: 0, clockwise: true)
    path.line(to: NSPoint(x: right, y: gapAt + 1))
    path.line(to: NSPoint(x: right - stem, y: gapAt + 1))
    path.line(to: NSPoint(x: right - stem, y: center.y))
    path.appendArc(
        withCenter: center, radius: radius - stem, startAngle: 0, endAngle: 180, clockwise: false)
    path.line(to: NSPoint(x: left + stem, y: top))
    path.close()
    path.appendRect(NSRect(x: right - stem, y: top, width: stem, height: gapAt - top))
    return path
}

/// Three dots: whole pixels on a standard display, round dots on Retina.
private func ellipsis(center: NSPoint, scale: CGFloat) {
    if scale < 1.5 {
        for dx: CGFloat in [-2, 0, 2] {
            NSBezierPath(rect: NSRect(x: center.x + dx - 0.5, y: center.y - 0.5, width: 1, height: 1))
                .fill()
        }
    } else {
        for dx: CGFloat in [-2.5, 0, 2.5] {
            NSBezierPath(
                ovalIn: NSRect(x: center.x + dx - 0.75, y: center.y - 0.75, width: 1.5, height: 1.5)
            ).fill()
        }
    }
}

/// Device pixels per point of the context being drawn into, so stems land on
/// whole pixels on both standard and Retina displays.
private func pixelScale() -> CGFloat {
    guard let t = NSGraphicsContext.current?.cgContext.userSpaceToDeviceSpaceTransform else {
        return 2
    }
    return max(1, (t.a * t.a + t.b * t.b).squareRoot().rounded())
}
