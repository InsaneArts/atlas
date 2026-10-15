import AppKit
import AtlasKit
import Combine
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
    let closed: @MainActor () -> Void
    private var watching: AnyCancellable?

    public init(model: OnboardingModel, closed: @escaping @MainActor () -> Void) {
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
        let host = NSHostingView(
            rootView: OnboardingView(model: model) { [weak self] in self?.window.close() })
        host.sizingOptions = []
        window.contentView = host
        model.close = { [weak self] in self?.window.close() }
        watching = model.$guiding
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] pane in self?.place(guiding: pane != nil) }
    }

    public func show() {
        NSApp.setActivationPolicy(.regular)
        window.setContentSize(Self.card)
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        model.start()
    }

    /// The card in the middle, or the side panel at the left of the screen
    /// (where System Settings leaves room), floating so it stays in view.
    func place(guiding: Bool) {
        guard let screen = window.screen ?? NSScreen.main else { return }
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

    public func windowWillClose(_ notification: Notification) {
        model.closing()
        NSApp.setActivationPolicy(.accessory)
        closed()
    }
}

/// Draw every onboarding screen, in fixed states, to PNGs in `directory`
/// at Retina scale, for checking against the design without a real setup.
@MainActor
public func renderOnboarding(to directory: String) {
    func controller(_ helper: [String: Any]?, finished: Bool = false, candidates: Bool = true) -> OnboardingFacts {
        var reply: [String: Any] = [
            "role": "controller", "me": ["name": "theiskaa", "realName": "Ismayil"],
            "app": ["installed": true, "signed": true, "linked": true],
            "configured": helper != nil, "finished": finished,
            "candidates": candidates ? [["name": "testaccount", "realName": "Test Account"]] : [],
        ]
        if let helper { reply["helper"] = helper }
        return OnboardingFacts(reply)
    }
    func helper(signedIn: Bool = true, answering: Bool = true, hidden: Bool = true, allowed: Bool = true) -> [String: Any] {
        [
            "exists": true, "name": "atlas", "realName": "Atlas Helper", "administrator": false,
            "leftoverSession": false, "paired": true, "signedIn": signedIn, "answering": answering,
            "onConsole": !hidden, "hidden": hidden,
            "permissions": ["screenCapture": allowed, "accessibility": allowed],
        ]
    }
    func asker(running: Bool, ax: Bool, screen: Bool) -> OnboardingFacts {
        OnboardingFacts([
            "role": "helper", "me": ["name": "atlas", "realName": "Atlas Helper"],
            "app": ["installed": true, "signed": true],
            "helper": [
                "controller": ["uid": 501, "name": "theiskaa", "realName": "Ismayil"],
                "installed": running, "running": running,
                "permissions": ["screenCapture": screen, "accessibility": ax],
            ],
        ])
    }
    var started = OnboardingChoices()
    started.started = true
    var creating = started
    creating.creating = true
    var agentsDone = OnboardingChoices()
    agentsDone.agentsDone = true

    var screens: [(String, OnboardingModel, NSSize)] = []
    let card = NSSize(width: 720, height: 540)
    func add(_ name: String, _ model: OnboardingModel, _ size: NSSize? = nil) {
        screens.append((name, model, size ?? card))
    }
    add("01-welcome", .preview(controller(nil)))
    let choose = OnboardingModel.preview(controller(nil), started)
    choose.picked = ""
    add("02-choose", choose)
    add("03-create", .preview(controller(nil), creating))
    let creatingNow = OnboardingModel.preview(controller(nil), creating)
    creatingNow.setForPreview(working: "Creating Atlas Helper… Confirm in the macOS dialog.")
    add("03-create-working", creatingNow)
    add("04-switch", .preview(controller(helper(signedIn: false, answering: false, allowed: false))))
    add("04-switch-open-atlas", .preview(controller(helper(answering: false, allowed: false))))
    let finishing = OnboardingModel.preview(controller(helper()))
    finishing.setForPreview(finish: .running)
    add("05-finishing", finishing)
    let passed = OnboardingModel.preview(controller(helper()))
    passed.setForPreview(finish: .passed)
    add("05-finishing-passed", passed)
    let agents = OnboardingModel.preview(controller(helper(), finished: true))
    agents.agents = [
        .init(id: "claude", title: "Claude Code", found: true, connected: false, on: true),
        .init(id: "codex", title: "Codex", found: true, connected: false, on: true),
    ]
    agents.projects = [URL(fileURLWithPath: "/Users/me/dev/Opie")]
    add("06-agents", agents)
    add("07-done", .preview(controller(helper(), finished: true), agentsDone))
    add("08-helper-allow", .preview(asker(running: false, ax: false, screen: false)))
    add("09-helper-permissions", .preview(asker(running: true, ax: true, screen: false)))
    let guide = OnboardingModel.preview(asker(running: true, ax: false, screen: false))
    guide.setForPreview(guiding: "Accessibility")
    add("10-helper-guide", guide, OnboardingWindow.side)
    add("11-helper-done", .preview(asker(running: true, ax: true, screen: true)))

    for (name, model, size) in screens {
        let host = NSHostingView(rootView: OnboardingView(model: model) {})
        host.frame = NSRect(origin: .zero, size: size)
        host.appearance = NSAppearance(named: .darkAqua)
        host.layoutSubtreeIfNeeded()
        guard
            let bitmap = NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: Int(size.width) * 2, pixelsHigh: Int(size.height) * 2,
                bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
        else { continue }
        bitmap.size = size
        host.cacheDisplay(in: host.bounds, to: bitmap)
        if let png = bitmap.representation(using: .png, properties: [:]) {
            try? png.write(to: URL(fileURLWithPath: directory).appendingPathComponent("\(name).png"))
        }
    }
}
