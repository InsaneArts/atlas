import AppKit
import AtlasKit
import SwiftUI

/// The viewer's window: dark, resizable, its controls in the title bar. It
/// reads only while on screen, and gives control back whenever it closes,
/// hides or loses focus.
@MainActor
public final class ViewerWindow: NSObject, NSWindowDelegate {
    let model: ViewerModel
    let window: NSWindow
    let closed: @MainActor (ViewerWindow) -> Void
    /// Open until closed, minimised included.
    public private(set) var isOpen = false

    public init(model: ViewerModel, closed: @escaping @MainActor (ViewerWindow) -> Void) {
        self.model = model
        self.closed = closed
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered, defer: false)
        super.init()
        window.title = "Atlas Viewer"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.titlebarSeparatorStyle = .none
        // An empty toolbar gives the title bar the header's height, so the
        // window buttons line up with it.
        window.toolbar = NSToolbar(identifier: "viewer")
        window.toolbarStyle = .unified
        window.isReleasedWhenClosed = false
        window.appearance = NSAppearance(named: .darkAqua)
        window.backgroundColor = AtlasStyle.groundColor
        window.contentMinSize = NSSize(width: 760, height: 520)
        window.collectionBehavior.insert(.fullScreenPrimary)
        window.delegate = self
        let host = NSHostingView(rootView: ViewerView(model: model))
        // The window keeps the size it is given; the content fills it.
        host.sizingOptions = []
        window.contentView = host
        window.setFrameAutosaveName("AtlasViewer")
    }

    public var isKey: Bool { window.isKeyWindow }

    public func show() {
        NSApp.setActivationPolicy(.regular)
        if !isOpen && !window.setFrameUsingName("AtlasViewer") { window.center() }
        isOpen = true
        if window.isMiniaturized { window.deminiaturize(nil) }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
        if window.isKeyWindow { model.gainedFocus() } else { model.lostFocus() }
        model.visible(true)
    }

    public func windowWillEnterFullScreen(_ notification: Notification) { model.fullScreen = true }
    public func windowDidExitFullScreen(_ notification: Notification) { model.fullScreen = false }

    public func close() { window.close() }

    // Off screen (minimised, covered, or on another Space): stop reading
    // and give control back; on screen again: read again.
    public func windowDidChangeOcclusionState(_ notification: Notification) {
        model.visible(window.occlusionState.contains(.visible) && window.isVisible)
    }

    public func windowDidBecomeKey(_ notification: Notification) {
        model.gainedFocus()
    }

    public func windowDidResignKey(_ notification: Notification) {
        model.lostFocus()
    }

    public func windowWillClose(_ notification: Notification) {
        isOpen = false
        model.close()
        Task { closed(self) }
    }
}
