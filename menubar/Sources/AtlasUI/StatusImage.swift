import AppKit
import AtlasKit

/// The menu bar glyph: a monochrome template "A" whose crossbar floats free
/// of its legs, the load Atlas carries. Marks take the crossbar's place;
/// control turns it into a solid tile with the letter cut out. The shape,
/// not a colour, carries the state, like the system's own items.
public func statusImage(_ mode: Mode) -> NSImage {
    let image = NSImage(size: NSSize(width: 15, height: 16), flipped: true) { _ in
        let scale = pixelScale()
        // A heavier stroke on a standard display, a lighter one on Retina.
        let stem: CGFloat = scale < 1.5 ? 2 : 1.5
        let ink = NSColor.black.withAlphaComponent(mode.mark == .dimmed ? 0.4 : 1)
        ink.set()
        guard mode.mark != .filled else {
            NSBezierPath(
                roundedRect: NSRect(x: 0, y: 0, width: 15, height: 16), xRadius: 4, yRadius: 4
            ).fill()
            NSGraphicsContext.current?.compositingOperation = .clear
            let small = LetterA(left: 3, right: 12, top: 3, bottom: 13, apex: 1.6, leg: 1.8)
            small.legs.fill()
            small.bar(at: 9, height: 1.5, gap: 0.6).fill()
            NSGraphicsContext.current?.compositingOperation = .sourceOver
            return true
        }
        let letter = LetterA(
            left: 1, right: 14, top: 1, bottom: 15, apex: 2, leg: scale < 1.5 ? 2 : 1.75)
        letter.legs.fill()
        switch mode.mark {
        case .working:
            ellipsis(center: NSPoint(x: 7.5, y: 12.5), scale: scale)
        case .alert:
            NSBezierPath(rect: NSRect(x: 7, y: 6, width: 1, height: 4)).fill()
            NSBezierPath(rect: NSRect(x: 7, y: 11, width: 1, height: 1)).fill()
        case .dash:
            NSBezierPath(rect: NSRect(x: 5.5, y: 12, width: 4, height: 1)).fill()
        default:
            letter.bar(at: 10, height: stem, gap: 0.8).fill()
        }
        return true
    }
    image.isTemplate = true
    image.accessibilityDescription = "Atlas: \(mode.label)"
    return image
}

/// An "A" in a flipped (top-down) box: two slanted legs meeting in a flat
/// apex, and a crossbar drawn apart from them.
struct LetterA {
    let left: CGFloat, right: CGFloat, top: CGFloat, bottom: CGFloat
    /// The flat top's width, and each leg's horizontal thickness.
    let apex: CGFloat, leg: CGFloat

    var center: CGFloat { (left + right) / 2 }

    /// Where the inside of the left leg is at height `y`.
    func inside(at y: CGFloat) -> CGFloat {
        left + leg + (center - apex / 2 - left) * (bottom - y) / (bottom - top)
    }

    var legs: NSBezierPath {
        // The inner edges meet below the apex, by symmetry on the centre line.
        let reach = (center - left - leg) / (center - apex / 2 - left)
        let notch = bottom - reach * (bottom - top)
        let path = NSBezierPath()
        path.move(to: NSPoint(x: left, y: bottom))
        path.line(to: NSPoint(x: center - apex / 2, y: top))
        path.line(to: NSPoint(x: center + apex / 2, y: top))
        path.line(to: NSPoint(x: right, y: bottom))
        path.line(to: NSPoint(x: right - leg, y: bottom))
        path.line(to: NSPoint(x: center, y: notch))
        path.line(to: NSPoint(x: left + leg, y: bottom))
        path.close()
        return path
    }

    /// The crossbar from `y` down by `height`, `gap` clear of each leg.
    func bar(at y: CGFloat, height: CGFloat, gap: CGFloat) -> NSBezierPath {
        let from = inside(at: y) + gap
        return NSBezierPath(rect: NSRect(x: from, y: y, width: 2 * center - 2 * from, height: height))
    }
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
