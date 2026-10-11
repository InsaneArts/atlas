import AppKit
import AtlasKit

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

extension NSView {
    var isDark: Bool { effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua }
}

/// A small filled circle in the mode's colour.
public final class DotView: NSView {
    public var color = NSColor.tertiaryLabelColor { didSet { needsDisplay = true } }

    public override var intrinsicContentSize: NSSize { NSSize(width: 7, height: 7) }

    public override func draw(_ dirtyRect: NSRect) {
        color.setFill()
        NSBezierPath(ovalIn: bounds).fill()
    }
}

/// The helper desktop, letterboxed on a black rounded stage.
public final class ThumbnailView: NSView {
    public var image: NSImage? { didSet { needsDisplay = true } }
    public var placeholder = "Waiting for the helper’s screen…" { didSet { needsDisplay = true } }

    public override var intrinsicContentSize: NSSize { NSSize(width: 312, height: 176) }

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
            drawPlaceholder()
        }
        NSGraphicsContext.restoreGraphicsState()
        // A hairline edge keeps the stage crisp against the popover.
        let edge = NSBezierPath(
            roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 9.5, yRadius: 9.5)
        edge.lineWidth = 1
        NSColor(white: isDark ? 1 : 0, alpha: isDark ? 0.12 : 0.1).setStroke()
        edge.stroke()
    }

    private func drawPlaceholder() {
        let ink = NSColor(white: 1, alpha: 0.45)
        let text = NSAttributedString(
            string: placeholder,
            attributes: [.font: NSFont.systemFont(ofSize: 11), .foregroundColor: ink])
        let textSize = text.size()
        let symbol = NSImage(systemSymbolName: "display", accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 26, weight: .light))
        let symbolSize = symbol?.size ?? .zero
        let gap: CGFloat = symbol == nil ? 0 : 8
        let total = symbolSize.height + gap + textSize.height
        let bottom = (bounds.height - total) / 2
        text.draw(at: NSPoint(x: (bounds.width - textSize.width) / 2, y: bottom))
        guard let symbol else { return }
        // Tint the template symbol in its own layer, then place it.
        let tinted = NSImage(size: symbolSize, flipped: false) { rect in
            symbol.draw(in: rect)
            ink.set()
            rect.fill(using: .sourceAtop)
            return true
        }
        tinted.draw(
            in: NSRect(
                x: (bounds.width - symbolSize.width) / 2, y: bottom + textSize.height + gap,
                width: symbolSize.width, height: symbolSize.height))
    }
}

/// A grouped block of rows on a rounded background, as in System Settings.
public final class GroupView: NSView {
    public override func draw(_ dirtyRect: NSRect) {
        let shape = NSBezierPath(
            roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 9.5, yRadius: 9.5)
        NSColor(white: 1, alpha: isDark ? 0.06 : 0.75).setFill()
        shape.fill()
        NSColor(white: isDark ? 1 : 0, alpha: isDark ? 0.08 : 0.07).setStroke()
        shape.lineWidth = 1
        shape.stroke()
    }
}

/// A one-pixel separator line.
public final class Hairline: NSView {
    public override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: 1)
    }

    public override func draw(_ dirtyRect: NSRect) {
        let scale = window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2
        NSColor.separatorColor.setFill()
        NSRect(x: 0, y: 0, width: bounds.width, height: 1 / scale).fill()
    }
}

func label(
    _ text: String, size: CGFloat = 13, weight: NSFont.Weight = .regular,
    color: NSColor = .labelColor
) -> NSTextField {
    let field = NSTextField(labelWithString: text)
    field.font = .systemFont(ofSize: size, weight: weight)
    field.textColor = color
    field.lineBreakMode = .byTruncatingTail
    return field
}

/// One row of a group: a name on the left, its value on the right.
func infoRow(_ name: String, _ value: NSTextField) -> NSStackView {
    let title = label(name)
    value.textColor = .secondaryLabelColor
    value.alignment = .right
    title.setContentCompressionResistancePriority(.required, for: .horizontal)
    title.setContentHuggingPriority(.required, for: .horizontal)
    value.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    value.setContentHuggingPriority(.defaultLow, for: .horizontal)
    let line = NSStackView(views: [title, value])
    line.distribution = .fill
    line.spacing = 12
    line.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
    return line
}

/// A row's round symbol, as in Control Center: grey, or filled with the
/// accent when the row is the one that matters now.
final class IconBadge: NSView {
    let glyph = NSImageView()
    var emphasized = false { didSet { refresh() } }
    var enabled = true { didSet { refresh() } }

    init(symbol: String) {
        super.init(frame: NSRect(x: 0, y: 0, width: 26, height: 26))
        glyph.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 12, weight: .medium))
        glyph.imageScaling = .scaleNone
        glyph.translatesAutoresizingMaskIntoConstraints = false
        addSubview(glyph)
        NSLayoutConstraint.activate([
            glyph.centerXAnchor.constraint(equalTo: centerXAnchor),
            glyph.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        refresh()
    }

    required init?(coder: NSCoder) { nil }

    override var intrinsicContentSize: NSSize { NSSize(width: 26, height: 26) }

    func refresh() {
        let accent = emphasized && enabled
        glyph.contentTintColor =
            accent ? .white : enabled ? .labelColor : .tertiaryLabelColor
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        if emphasized && enabled {
            NSColor.controlAccentColor.setFill()
        } else {
            let alpha: CGFloat = isDark ? 0.14 : 0.08
            NSColor(white: isDark ? 1 : 0, alpha: enabled ? alpha : alpha / 2).setFill()
        }
        NSBezierPath(ovalIn: bounds).fill()
    }
}

/// A full-width, menu-style row: an optional round symbol, a title, and a
/// trailing detail (a name or shortcut). It highlights under the pointer like
/// the rows in Control Center and runs `press` when clicked.
public final class RowButton: NSView {
    public var isEnabled = true { didSet { refresh() } }
    public var isEmphasized = false { didSet { badge?.emphasized = isEmphasized } }
    public var detail: String {
        get { detailField.stringValue }
        set {
            detailField.stringValue = newValue
            detailField.isHidden = newValue.isEmpty
        }
    }
    var keyEquivalent = ""
    var keyEquivalentModifierMask: NSEvent.ModifierFlags = []
    var press: (() -> Void)?

    private let badge: IconBadge?
    private let titleField: NSTextField
    private let detailField = label("", color: .tertiaryLabelColor)
    private var hovering = false { didSet { needsDisplay = true } }
    private var pressing = false { didSet { needsDisplay = true } }

    init(_ title: String, symbol: String? = nil, detail: String = "") {
        badge = symbol.map(IconBadge.init(symbol:))
        titleField = label(title)
        super.init(frame: .zero)
        let views: [NSView] = (badge.map { [$0] } ?? []) + [titleField, NSView(), detailField]
        let stack = NSStackView(views: views)
        stack.spacing = 10
        stack.edgeInsets = NSEdgeInsets(top: 0, left: 8, bottom: 0, right: 10)
        stack.translatesAutoresizingMaskIntoConstraints = false
        titleField.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        detailField.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
            heightAnchor.constraint(equalToConstant: symbol == nil ? 24 : 34),
        ])
        self.detail = detail
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
        setAccessibilityLabel(title)
        // Keyboard focus shows as the hover highlight, not a blue ring.
        focusRingType = .none
        refresh()
    }

    required init?(coder: NSCoder) { nil }

    private func refresh() {
        titleField.textColor = isEnabled ? .labelColor : .tertiaryLabelColor
        badge?.enabled = isEnabled
        setAccessibilityEnabled(isEnabled)
        if !isEnabled {
            hovering = false
            pressing = false
        }
    }

    public override func draw(_ dirtyRect: NSRect) {
        guard isEnabled, hovering || pressing || focused else { return }
        NSColor(white: isDark ? 1 : 0, alpha: pressing ? 0.14 : 0.07).setFill()
        NSBezierPath(roundedRect: bounds, xRadius: 6, yRadius: 6).fill()
    }

    public override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                owner: self))
    }

    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        hovering = false
        pressing = false
    }

    public override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    // Keyboard: Tab reaches each row, Space or Return activates it, and the
    // focused row is highlighted as if under the pointer.
    public override var acceptsFirstResponder: Bool { isEnabled }
    public override var canBecomeKeyView: Bool { isEnabled }
    private var focused: Bool { window?.firstResponder === self }

    public override func becomeFirstResponder() -> Bool {
        needsDisplay = true
        return super.becomeFirstResponder()
    }

    public override func resignFirstResponder() -> Bool {
        needsDisplay = true
        return super.resignFirstResponder()
    }

    public override func keyDown(with event: NSEvent) {
        if isEnabled, [" ", "\r"].contains(event.charactersIgnoringModifiers ?? "") {
            activate()
        } else {
            super.keyDown(with: event)
        }
    }

    public override func mouseEntered(with event: NSEvent) { hovering = isEnabled }
    public override func mouseExited(with event: NSEvent) { hovering = false }

    public override func mouseDown(with event: NSEvent) {
        pressing = isEnabled
    }

    public override func mouseDragged(with event: NSEvent) {
        guard isEnabled else { return }
        pressing = bounds.contains(convert(event.locationInWindow, from: nil))
    }

    public override func mouseUp(with event: NSEvent) {
        let inside = bounds.contains(convert(event.locationInWindow, from: nil))
        pressing = false
        if isEnabled && inside { activate() }
    }

    public override func performKeyEquivalent(with event: NSEvent) -> Bool {
        // Return belongs to whichever row has keyboard focus.
        if let focused = window?.firstResponder as? RowButton, focused !== self,
            keyEquivalentModifierMask.isEmpty
        {
            return super.performKeyEquivalent(with: event)
        }
        let modifiers = event.modifierFlags.intersection([.command, .option, .control, .shift])
        if isEnabled, !isHiddenOrHasHiddenAncestor, !keyEquivalent.isEmpty,
            event.charactersIgnoringModifiers == keyEquivalent,
            modifiers == keyEquivalentModifierMask
        {
            activate()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    public override func accessibilityPerformPress() -> Bool {
        guard isEnabled else { return false }
        activate()
        return true
    }

    private func activate() { press?() }
}
