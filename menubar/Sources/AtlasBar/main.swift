import AppKit
import AtlasKit
import AtlasUI

// Atlas's menu bar app. The viewer's capability URL arrives on stdin,
// never in argv, and the app quits when stdin closes, so it never outlives
// the `atlas menubar` that started it.

final class Companion: NSObject, NSApplicationDelegate, NSPopoverDelegate {
    let viewer: Viewer
    let commands: Commands
    let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    let popover = NSPopover()
    let panel: PanelController
    var timer: Timer?
    // One request of each kind at a time, so a slow reply can never land
    // after, and overwrite, a newer one.
    var statusPending = false
    var framePending = false

    init(viewer: Viewer, commands: Commands) {
        self.viewer = viewer
        self.commands = commands
        panel = PanelController(viewer: viewer, commands: commands, loginItem: LoginItem())
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        popover.contentViewController = panel
        popover.behavior = .transient
        // Atlas's panel is always dark, like its other windows.
        popover.appearance = NSAppearance(named: .darkAqua)
        popover.delegate = self
        item.button?.image = statusImage(Mode.of("unavailable"))
        item.button?.target = self
        item.button?.action = #selector(toggle)
        // The helper to switch to is whichever `atlas` reports.
        commands.run(["switch", "--list"]) { [weak self] outcome in
            let accounts = outcome.reply["accounts"] as? [[String: Any]] ?? []
            self?.panel.helper =
                accounts.first { $0["helper"] as? Bool == true }?["name"] as? String
        }
        schedule(open: false)
    }

    @objc func toggle() {
        guard let button = item.button else { return }
        if popover.isShown {
            popover.performClose(nil)
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            // No row is focused until the person presses Tab, as in menus;
            // again once the popover has become key and picked its own.
            let window = popover.contentViewController?.view.window
            window?.makeFirstResponder(nil)
            DispatchQueue.main.async { window?.makeFirstResponder(nil) }
            schedule(open: true)
        }
    }

    func popoverDidClose(_ notification: Notification) { schedule(open: false) }

    /// Poll status (and the desktop while open): every 2 s open, 5 s closed.
    func schedule(open: Bool) {
        timer?.invalidate()
        refresh(withFrame: open)
        timer = Timer.scheduledTimer(withTimeInterval: open ? 2 : 5, repeats: true) {
            [weak self] _ in
            self?.refresh(withFrame: open)
        }
    }

    func refresh(withFrame: Bool) {
        if !statusPending {
            statusPending = true
            viewer.status { [weak self] status in
                guard let self else { return }
                statusPending = false
                let snapshot = status.map(Snapshot.init(status:)) ?? Snapshot()
                let mode = Mode.of(snapshot.mode)
                item.button?.image = statusImage(mode)
                item.button?.toolTip = "Atlas — \(mode.label)"
                panel.show(snapshot)
            }
        }
        if withFrame && !framePending {
            framePending = true
            viewer.frame { [weak self] frame in
                guard let self else { return }
                framePending = false
                // A failed capture never leaves an old desktop on show.
                panel.thumbnail.placeholder = "The helper’s screen is not available"
                panel.thumbnail.image = frame.flatMap(NSImage.init(data:))
            }
        }
    }
}

// A command that exits before reading what it was given must not end the
// app: writes to its closed pipe fail with an error instead.
signal(SIGPIPE, SIG_IGN)

/// The setup window's menus: Quit and Close, and Edit, so typing and
/// pasting work in its fields as in any app.
@MainActor
func onboardingMenu() -> NSMenu {
    let menu = NSMenu()
    let app = NSMenu(title: "Atlas")
    app.addItem(withTitle: "Hide Atlas", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
    app.addItem(.separator())
    app.addItem(withTitle: "Quit Atlas", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
    let edit = NSMenu(title: "Edit")
    edit.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
    let redo = edit.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
    redo.keyEquivalentModifierMask = [.command, .shift]
    edit.addItem(.separator())
    edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
    edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
    edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
    edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
    let window = NSMenu(title: "Window")
    window.addItem(withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
    window.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
    for submenu in [app, edit, window] {
        let item = NSMenuItem()
        item.submenu = submenu
        menu.addItem(item)
    }
    return menu
}

/// Quit when the parent closes stdin, so the bar never outlives the server.
func watchParent() {
    Thread.detachNewThread {
        while readLine() != nil {}
        DispatchQueue.main.async { NSApp.terminate(nil) }
    }
}

// `--notice TITLE MESSAGE`: tell the person something in this session (the
// helper's, when its daemon asks), wait for OK, then exit.
if let index = CommandLine.arguments.firstIndex(of: "--notice"),
    index + 2 < CommandLine.arguments.count
{
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    let alert = NSAlert()
    alert.messageText = CommandLine.arguments[index + 1]
    alert.informativeText = CommandLine.arguments[index + 2]
    alert.addButton(withTitle: "OK")
    // Finish launching first, so the request to come forward is honoured and
    // the notice has keyboard focus (Return closes it).
    app.finishLaunching()
    app.activate(ignoringOtherApps: true)
    alert.window.level = .floating
    alert.runModal()
    exit(0)
}

// `--login on|off|status`: manage "Open at Login" for the CLI, then exit.
if let index = CommandLine.arguments.firstIndex(of: "--login"),
    index + 1 < CommandLine.arguments.count
{
    let item = LoginItem()
    do {
        switch CommandLine.arguments[index + 1] {
        case "on": try item.set(true)
        case "off": try item.set(false)
        default: break
        }
        print("{\"ok\": true, \"enabled\": \(item.enabled), \"available\": \(item.available)}")
        exit(0)
    } catch {
        let message = error.localizedDescription.replacingOccurrences(of: "\"", with: "'")
        print("{\"ok\": false, \"error\": \"\(message)\"}")
        exit(1)
    }
}

// `--onboarding`: the setup window, on its own (Atlas.app opened before it
// is set up). It quits when the window closes.
if CommandLine.arguments.contains("--onboarding") {
    let app = NSApplication.shared
    let executable = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath()
    let window = MainActor.assumeIsolated {
        app.mainMenu = onboardingMenu()
        let model = OnboardingModel(commands: .parent(fallback: executable))
        let window = OnboardingWindow(model: model) { NSApp.terminate(nil) }
        app.setActivationPolicy(.regular)
        app.finishLaunching()
        window.show()
        return window
    }
    withExtendedLifetime(window) { app.run() }
    exit(0)
}

// `--render-onboarding DIR`: draw every onboarding screen to PNGs, then exit.
if let index = CommandLine.arguments.firstIndex(of: "--render-onboarding"),
    index + 1 < CommandLine.arguments.count
{
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    MainActor.assumeIsolated { renderOnboarding(to: CommandLine.arguments[index + 1]) }
    exit(0)
}

guard let line = readLine(), let viewer = Viewer(line) else {
    FileHandle.standardError.write(Data("Expected the viewer URL on stdin\n".utf8))
    exit(2)
}
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let arguments = CommandLine.arguments
if let index = arguments.firstIndex(of: "--render"), index + 1 < arguments.count {
    let dark = arguments.contains("--dark")
    watchParent()
    render(viewer: viewer, to: arguments[index + 1], appearance: dark ? .darkAqua : .aqua)
    app.run()
} else {
    let executable = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath()
    let companion = Companion(viewer: viewer, commands: .parent(fallback: executable))
    app.delegate = companion
    watchParent()
    app.run()
}
