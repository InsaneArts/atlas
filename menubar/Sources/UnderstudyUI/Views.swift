import AppKit
import UnderstudyKit

extension Mode.Tone {
    /// The dot's colour: system green when ready, the accent while working,
    /// orange when it needs the person, quiet grey otherwise.
    public var color: NSColor {
        switch self {
        case .ready: return .systemGreen
        case .active: return .controlAccentColor
        case .attention: return .systemOrange
        case .quiet: return .tertiaryLabelColor
        }
    }
}

/// A small filled circle in the mode's colour.
public final class DotView: NSView {
    public var color = NSColor.tertiaryLabelColor { didSet { needsDisplay = true } }

    public override var intrinsicContentSize: NSSize { NSSize(width: 6, height: 6) }

    public override func draw(_ dirtyRect: NSRect) {
        color.setFill()
        NSBezierPath(ovalIn: bounds).fill()
    }
}

/// The helper desktop, letterboxed on a black rounded stage.
public final class ThumbnailView: NSView {
    public var image: NSImage? { didSet { needsDisplay = true } }
    public var placeholder = "Waiting for the helper desktop…" { didSet { needsDisplay = true } }

    public override var intrinsicContentSize: NSSize { NSSize(width: 316, height: 178) }

    public override func draw(_ dirtyRect: NSRect) {
        let stage = NSBezierPath(roundedRect: bounds, xRadius: 10, yRadius: 10)
        NSColor.black.setFill()
        stage.fill()
        NSGraphicsContext.saveGraphicsState()
        stage.addClip()
        if let image, image.size.width > 0, image.size.height > 0 {
            let scale = min(bounds.width / image.size.width, bounds.height / image.size.height)
            let size = NSSize(width: image.size.width * scale, height: image.size.height * scale)
            let origin = NSPoint(
                x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2)
            image.draw(in: NSRect(origin: origin, size: size))
        } else {
            let text = NSAttributedString(
                string: placeholder,
                attributes: [
                    .font: NSFont.systemFont(ofSize: 11),
                    .foregroundColor: NSColor(white: 0.65, alpha: 1),
                ])
            let size = text.size()
            text.draw(
                at: NSPoint(
                    x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2))
        }
        NSGraphicsContext.restoreGraphicsState()
    }
}

/// A grouped block of rows on a rounded background, as in System Settings.
public final class GroupView: NSView {
    public override var wantsUpdateLayer: Bool { false }

    public override func draw(_ dirtyRect: NSRect) {
        let dark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        (dark ? NSColor(white: 1, alpha: 0.07) : NSColor.white).setFill()
        NSBezierPath(roundedRect: bounds, xRadius: 10, yRadius: 10).fill()
    }
}

func label(_ text: String, size: CGFloat = 13, color: NSColor = .labelColor) -> NSTextField {
    let field = NSTextField(labelWithString: text)
    field.font = .systemFont(ofSize: size)
    field.textColor = color
    field.lineBreakMode = .byTruncatingTail
    return field
}

/// One row of a group: a label on the left, a secondary value on the right.
func row(_ title: NSView, _ value: NSView) -> NSStackView {
    if let value = value as? NSTextField {
        value.textColor = .secondaryLabelColor
        value.alignment = .right
    }
    value.setContentCompressionResistancePriority(.required, for: .horizontal)
    title.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    let line = NSStackView(views: [title, NSView(), value])
    line.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
    return line
}

/// A plain, left-aligned menu-style button.
func menuButton(_ title: String, target: AnyObject, action: Selector) -> NSButton {
    let button = NSButton(title: title, target: target, action: action)
    button.isBordered = false
    button.alignment = .left
    return button
}
