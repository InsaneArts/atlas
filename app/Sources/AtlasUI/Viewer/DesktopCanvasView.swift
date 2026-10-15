import AppKit
import AtlasKit
import SwiftUI

/// The surface over the helper's picture that takes the mouse, scrolling
/// and the keyboard while this window holds control. Keys that name a key
/// (Return, arrows, shortcuts) go as keys; everything else goes through
/// the input method, so dead keys, accents and other scripts arrive as the
/// text they commit.
final class DesktopCanvasView: NSView, @preconcurrency NSTextInputClient {
    weak var model: ViewerModel?
    private var keys: Any?
    private var marked = NSAttributedString()
    /// The input method's selection within the marked text.
    private var markedSelection = NSRange(location: 0, length: 0)

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { model?.holdingControl == true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { false }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: .zero, options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil { stopIntercepting() }
    }

    /// Follow control: take the keyboard while it is held, give it up after.
    func follow() {
        guard let window, let model else { return }
        if model.holdingControl {
            startIntercepting()
            if window.firstResponder !== self { window.makeFirstResponder(self) }
        } else {
            stopIntercepting()
            if hasMarkedText() { unmarkText() }
            if window.firstResponder === self { window.makeFirstResponder(nil) }
        }
    }

    /// While in control, keys are seen before menus and the window's own
    /// shortcuts, so a shortcut reaches the helper rather than this app.
    private func startIntercepting() {
        guard keys == nil else { return }
        keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            // Local monitors run on the main thread.
            nonisolated(unsafe) let event = event
            nonisolated(unsafe) var passed: NSEvent? = event
            MainActor.assumeIsolated {
                // nil means handled here: AppKit then does nothing more with it.
                if let self { passed = self.intercept(event) }
            }
            return passed
        }
    }

    func stopIntercepting() {
        guard let keys else { return }
        NSEvent.removeMonitor(keys)
        self.keys = nil
    }

    override func becomeFirstResponder() -> Bool {
        model?.keyboardConnected = true
        return true
    }

    override func resignFirstResponder() -> Bool {
        model?.keyboardConnected = false
        return true
    }

    private func intercept(_ event: NSEvent) -> NSEvent? {
        guard let model, model.holdingControl, let window, window.isKeyWindow, window.firstResponder === self
        else { return event }
        let modifiers = Self.modifiers(event)
        // Giving control back works at any moment, mid-composition included.
        if KeyMap.isRelease(keyCode: event.keyCode, modifiers: modifiers) {
            if hasMarkedText() { unmarkText() }
            model.releaseControl()
            return nil
        }
        // Quitting stays with this Mac, by the key labelled Q in any layout.
        if modifiers == .command && event.charactersIgnoringModifiers?.lowercased() == "q" { return event }
        if hasMarkedText() {
            // Mid-composition, keys belong to the input method; a shortcut
            // ends the composition first.
            guard modifiers.contains(.command) else { return event }
            unmarkText()
        }
        let handled = model.keyPressed(keyCode: event.keyCode, modifiers: modifiers) {
            NSPasteboard.general.string(forType: .string)
        }
        if handled { return nil }
        // Other shortcuts are kept from this app; text goes on to keyDown.
        return modifiers.contains(.command) ? nil : event
    }

    static func modifiers(_ event: NSEvent) -> Modifiers {
        let flags = event.modifierFlags
        var modifiers: Modifiers = []
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        return modifiers
    }

    private func location(_ event: NSEvent) -> CGPoint { convert(event.locationInWindow, from: nil) }

    private static func button(_ event: NSEvent) -> MouseButton? {
        switch event.buttonNumber {
        case 0: .left
        case 1: .right
        case 2: .middle
        default: nil
        }
    }

    // MARK: Mouse

    override func mouseDown(with event: NSEvent) { down(event) }
    override func rightMouseDown(with event: NSEvent) { down(event) }
    override func otherMouseDown(with event: NSEvent) { down(event) }
    override func mouseDragged(with event: NSEvent) { dragged(event) }
    override func rightMouseDragged(with event: NSEvent) { dragged(event) }
    override func otherMouseDragged(with event: NSEvent) { dragged(event) }
    override func mouseUp(with event: NSEvent) { up(event) }
    override func rightMouseUp(with event: NSEvent) { up(event) }
    override func otherMouseUp(with event: NSEvent) { up(event) }

    private func down(_ event: NSEvent) {
        guard let model, model.holdingControl, window?.isKeyWindow == true, let button = Self.button(event) else {
            return
        }
        window?.makeFirstResponder(self)
        model.pointerDown(button, at: location(event), in: bounds.size, modifiers: Self.modifiers(event))
    }

    private func dragged(_ event: NSEvent) {
        guard let button = Self.button(event) else { return }
        model?.pointerDragged(button, to: location(event), in: bounds.size, modifiers: Self.modifiers(event))
    }

    private func up(_ event: NSEvent) {
        guard let button = Self.button(event) else { return }
        model?.pointerUp(button, at: location(event), in: bounds.size, modifiers: Self.modifiers(event))
    }

    override func mouseMoved(with event: NSEvent) {
        model?.pointerMoved(to: location(event), in: bounds.size)
    }

    override func scrollWheel(with event: NSEvent) {
        guard let model, model.holdingControl, window?.isKeyWindow == true else {
            return super.scrollWheel(with: event)
        }
        model.scrolled(
            dx: event.scrollingDeltaX, dy: event.scrollingDeltaY, precise: event.hasPreciseScrollingDeltas,
            at: location(event), in: bounds.size)
    }

    // A right click goes to the helper, never to a menu here.
    override func menu(for event: NSEvent) -> NSMenu? { nil }

    // MARK: Keyboard and text

    override func keyDown(with event: NSEvent) {
        guard model?.holdingControl == true else { return super.keyDown(with: event) }
        inputContext?.handleEvent(event)
    }

    func insertText(_ string: Any, replacementRange: NSRange) {
        let text = (string as? NSAttributedString)?.string ?? (string as? String) ?? ""
        marked = NSAttributedString()
        markedSelection = NSRange(location: 0, length: 0)
        model?.composing = ""
        model?.send(text: text)
    }

    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        marked = (string as? NSAttributedString) ?? NSAttributedString(string: (string as? String) ?? "")
        let end = marked.length
        let location = min(max(selectedRange.location == NSNotFound ? end : selectedRange.location, 0), end)
        markedSelection = NSRange(location: location, length: min(selectedRange.length, end - location))
        model?.composing = marked.string
    }

    func unmarkText() {
        marked = NSAttributedString()
        markedSelection = NSRange(location: 0, length: 0)
        model?.composing = ""
        inputContext?.discardMarkedText()
    }

    func hasMarkedText() -> Bool { marked.length > 0 }
    func markedRange() -> NSRange {
        hasMarkedText() ? NSRange(location: 0, length: marked.length) : NSRange(location: NSNotFound, length: 0)
    }
    func selectedRange() -> NSRange { markedSelection }
    func validAttributesForMarkedText() -> [NSAttributedString.Key] { [] }
    func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
        nil
    }
    func characterIndex(for point: NSPoint) -> Int { NSNotFound }

    /// The input method's candidate window sits at the desktop's bottom left.
    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        let corner = NSRect(x: bounds.minX + 16, y: bounds.maxY - 48, width: 1, height: 20)
        return window?.convertToScreen(convert(corner, to: nil)) ?? .zero
    }

    // A key the input method turns into a command after composing (Return
    // after a dead key, an arrow in Hangul) still goes as that key.
    override func doCommand(by selector: Selector) {
        guard let event = NSApp.currentEvent, event.type == .keyDown,
            KeyMap.action(keyCode: event.keyCode, modifiers: Self.modifiers(event)) != nil
        else { return }
        _ = model?.keyPressed(keyCode: event.keyCode, modifiers: Self.modifiers(event)) { nil }
    }
}

/// The canvas in SwiftUI.
struct DesktopInput: NSViewRepresentable {
    let model: ViewerModel

    func makeNSView(context: Context) -> DesktopCanvasView {
        let view = DesktopCanvasView()
        view.model = model
        view.setAccessibilityElement(true)
        view.setAccessibilityRole(.group)
        view.setAccessibilityLabel("Helper desktop input")
        return view
    }

    static func dismantleNSView(_ view: DesktopCanvasView, coordinator: ()) {
        view.stopIntercepting()
    }

    func updateNSView(_ view: DesktopCanvasView, context: Context) {
        view.model = model
        // Read here so SwiftUI calls this again when control changes.
        _ = model.holdingControl
        DispatchQueue.main.async { view.follow() }
    }
}

/// Common shortcuts and Escape, sent as keys to the helper.
struct Shortcuts: View {
    let model: ViewerModel

    static let items: [(String, String)] = [
        ("Copy ⌘C", "cmd+c"), ("Paste ⌘V", "cmd+v"), ("Select All ⌘A", "cmd+a"), ("Undo ⌘Z", "cmd+z"),
        ("Redo ⇧⌘Z", "cmd+shift+z"), ("Save ⌘S", "cmd+s"), ("Switch App ⌘⇥", "cmd+tab"),
    ]

    var body: some View {
        HStack(spacing: 4) {
            Menu {
                ForEach(Self.items, id: \.1) { item in
                    Button(item.0) { model.sendKey(item.1) }
                }
            } label: {
                Text("Shortcuts").font(.system(size: 13))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.visible)
            .fixedSize()
            .padding(.horizontal, 12)
            .frame(height: 34)
            .help("Send a shortcut to the helper")
            Button {
                model.sendKey("escape")
            } label: {
                Text("esc")
                    .font(.system(size: 11, weight: .semibold))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .overlay(RoundedRectangle(cornerRadius: 5).strokeBorder(AtlasStyle.ink2, lineWidth: 1))
                    .padding(.horizontal, 10)
                    .frame(height: 34)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Send Escape")
        }
        .foregroundStyle(AtlasStyle.ink2)
        .disabled(!model.holdingControl)
    }
}
