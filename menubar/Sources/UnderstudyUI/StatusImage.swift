import AppKit
import UnderstudyKit

/// The menu bar glyph: a monochrome template image of two stacked windows.
/// The shape, not a colour, carries the state, like the system's own items.
public func statusImage(_ mode: Mode) -> NSImage {
    let image = NSImage(size: NSSize(width: 20, height: 15), flipped: true) { _ in
        let ink = NSColor.black.withAlphaComponent(mode.mark == .dimmed ? 0.4 : 1)
        let back = NSBezierPath(
            roundedRect: NSRect(x: 5.5, y: 0.9, width: 13.6, height: 9.4), xRadius: 2.2,
            yRadius: 2.2)
        let front = NSBezierPath(
            roundedRect: NSRect(x: 0.9, y: 4.7, width: 13.6, height: 9.4), xRadius: 2.2,
            yRadius: 2.2)
        ink.setStroke()
        back.lineWidth = 1.3
        back.stroke()
        // Clear the back window behind the front one, then draw the front.
        NSGraphicsContext.current?.compositingOperation = .clear
        front.fill()
        NSGraphicsContext.current?.compositingOperation = .sourceOver
        ink.set()
        front.lineWidth = 1.3
        if mode.mark == .filled { front.fill() } else { front.stroke() }
        switch mode.mark {
        case .working:
            for x in [4.6, 7.7, 10.8] {
                NSBezierPath(ovalIn: NSRect(x: x - 0.8, y: 8.6, width: 1.6, height: 1.6)).fill()
            }
        case .alert:
            let line = NSBezierPath()
            line.move(to: NSPoint(x: 7.7, y: 7.2))
            line.line(to: NSPoint(x: 7.7, y: 9.6))
            line.lineWidth = 1.4
            line.lineCapStyle = .round
            line.stroke()
            NSBezierPath(ovalIn: NSRect(x: 6.95, y: 10.9, width: 1.5, height: 1.5)).fill()
        case .dash:
            let line = NSBezierPath()
            line.move(to: NSPoint(x: 5.6, y: 9.4))
            line.line(to: NSPoint(x: 9.8, y: 9.4))
            line.lineWidth = 1.4
            line.lineCapStyle = .round
            line.stroke()
        default:
            break
        }
        return true
    }
    image.isTemplate = true
    image.accessibilityDescription = "Understudy: \(mode.label)"
    return image
}
