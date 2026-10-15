import AppKit
import AtlasKit
import SwiftUI

/// The onboarding window: a dark card in the middle of the screen, or a
/// narrow panel at its left while the person turns a switch on in System
/// Settings beside it. It has a Dock icon while open, like any app window.
@MainActor
public final class OnboardingWindow: NSObject, NSWindowDelegate {
    public static let card = NSSize(width: 720, height: 540)
    public static let side = NSSize(width: 360, height: 500)

    let model: OnboardingModel
    let window: NSWindow
    let closed: @MainActor (OnboardingWindow) -> Void

    /// `closed` runs once the window has closed and its model has stopped.
    public init(model: OnboardingModel, closed: @escaping @MainActor (OnboardingWindow) -> Void) {
        self.model = model
        self.closed = closed
        window = NSWindow(
            contentRect: NSRect(origin: .zero, size: Self.card),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered, defer: false)
        super.init()
        window.title = "Atlas"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isReleasedWhenClosed = false
        window.isMovableByWindowBackground = true
        window.appearance = NSAppearance(named: .darkAqua)
        window.backgroundColor = AtlasStyle.groundColor
        window.delegate = self
        let host = NSHostingView(rootView: OnboardingView(model: model))
        host.sizingOptions = []
        window.contentView = host
        model.close = { [weak self] in self?.window.close() }
        track({ [weak model] in model.map { $0.guiding != nil } }) { [weak self] guiding in
            self?.place(guiding: guiding)
        }
    }

    /// Bring the window forward, reading the facts while it is open.
    public func show() {
        NSApp.setActivationPolicy(.regular)
        if !isOpen {
            window.setContentSize(Self.card)
            window.center()
        }
        isOpen = true
        if window.isMiniaturized { window.deminiaturize(nil) }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
        model.start()
    }

    /// The card in the middle, or the side panel at the left of the screen
    /// (where System Settings leaves room), floating so it stays in view.
    func place(guiding: Bool) {
        guard window.isVisible, let screen = window.screen ?? NSScreen.main else { return }
        let visible = screen.visibleFrame
        let size = guiding ? Self.side : Self.card
        let frame = window.frameRect(forContentRect: NSRect(origin: .zero, size: size))
        let origin =
            guiding
            ? NSPoint(x: visible.minX + 40, y: visible.midY - frame.height / 2)
            : NSPoint(x: visible.midX - frame.width / 2, y: visible.midY - frame.height / 2)
        window.level = guiding ? .floating : .normal
        NSAnimationContext.runAnimationGroup { context in
            context.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 0.45
            context.timingFunction = CAMediaTimingFunction(controlPoints: 0.65, 0, 0.35, 1)
            window.animator().setFrame(NSRect(origin: origin, size: frame.size), display: true)
        }
    }

    /// Open until closed, minimised included.
    public private(set) var isOpen = false
    /// Whether the window takes the keyboard now.
    public var isKey: Bool { window.isKeyWindow }

    public func close() { window.close() }

    /// The app settles its Dock icon once `closed` has run.
    public func windowWillClose(_ notification: Notification) {
        isOpen = false
        Task {
            await model.closing()
            closed(self)
        }
    }
}
