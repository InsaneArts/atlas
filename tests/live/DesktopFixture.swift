import AppKit
import CoreGraphics

struct FixtureState: Encodable {
    let uid = getuid()
    var clicks = 0
    var doubleClicks = 0
    var rightClicks = 0
    var moves = 0
    var dragEvents = 0
    var mouseHeld = false
    var mouseUps = 0
    var modifierFlags: UInt64 = 0
    var selectionLength = 0
    var windowX = 0.0
    var windowY = 0.0
    var windowWidth = 0.0
    var windowHeight = 0.0
    var dotX = 80.0
    var dotY = 100.0
    var scrollX = 0.0
    var scrollY = 0.0
    var menuCount = 0
    var dialogCount = 0
    var dialogResponse = 0
    var ticks = 0
    var text = ""
    var savedPath: String?
    var saveError: String?
}

final class Recorder {
    let directory: URL
    var state = FixtureState()
    private let encoder = JSONEncoder()
    private let formatter = ISO8601DateFormatter()

    init(directory: URL) throws {
        self.directory = directory
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    func record(_ kind: String, values: [String: Any] = [:]) {
        do {
            var entry = values
            entry["event"] = kind
            entry["time"] = formatter.string(from: Date())
            entry["uptime"] = ProcessInfo.processInfo.systemUptime
            entry["sessionOnConsole"] =
                (CGSessionCopyCurrentDictionary() as? [String: Any])?["kCGSSessionOnConsoleKey"]
                ?? NSNull()
            var data = try JSONSerialization.data(withJSONObject: entry, options: [.sortedKeys])
            data.append(10)
            let events = directory.appendingPathComponent("events.jsonl")
            if !FileManager.default.fileExists(atPath: events.path) {
                FileManager.default.createFile(atPath: events.path, contents: nil)
            }
            let file = try FileHandle(forWritingTo: events)
            defer { try? file.close() }
            try file.seekToEnd()
            try file.write(contentsOf: data)
            let snapshot = directory.appendingPathComponent("state.json")
            try encoder.encode(state).write(to: snapshot, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o644], ofItemAtPath: snapshot.path)
        } catch {
            fputs("Fixture could not record evidence: \(error)\n", stderr)
            NSApp.terminate(nil)
        }
    }
}

final class PointerTarget: NSView {
    let recorder: Recorder
    override var isFlipped: Bool { true }

    init(frame: NSRect, recorder: Recorder) {
        self.recorder = recorder
        super.init(frame: frame)
    }

    required init?(coder: NSCoder) { nil }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: bounds, options: [.mouseMoved, .activeAlways, .inVisibleRect], owner: self))
    }

    override func draw(_ rect: NSRect) {
        NSColor(calibratedWhite: 0.92, alpha: 1).setFill()
        bounds.fill()
        let heading = "CLICK / DOUBLE CLICK / DRAG / SCROLL" as NSString
        heading.draw(
            at: NSPoint(x: 15, y: 15), withAttributes: [.font: NSFont.systemFont(ofSize: 18)])
        NSColor.systemOrange.setFill()
        NSBezierPath(
            ovalIn: NSRect(
                x: recorder.state.dotX - 20, y: recorder.state.dotY - 20, width: 40, height: 40)
        ).fill()
        let state = recorder.state
        let summary =
            "Clicks: \(state.clicks)   Doubles: \(state.doubleClicks)   Drags: \(state.dragEvents)\nScroll X: \(state.scrollX)   Y: \(state.scrollY)"
            as NSString
        summary.draw(
            at: NSPoint(x: 15, y: 180),
            withAttributes: [.font: NSFont.monospacedSystemFont(ofSize: 16, weight: .regular)])
    }

    override func mouseMoved(with event: NSEvent) {
        recorder.state.moves += 1
        recorder.record("mouseMoved")
    }

    override func mouseDown(with event: NSEvent) {
        recorder.state.mouseHeld = true
        recorder.state.clicks += 1
        if event.clickCount == 2 { recorder.state.doubleClicks += 1 }
        recorder.record(
            "mouseDown",
            values: [
                "clickCount": event.clickCount, "x": event.locationInWindow.x,
                "y": event.locationInWindow.y,
            ])
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        recorder.state.dragEvents += 1
        recorder.state.dotX = point.x
        recorder.state.dotY = point.y
        recorder.record(
            "drag", values: ["x": point.x, "y": point.y, "dx": event.deltaX, "dy": event.deltaY])
        needsDisplay = true
    }

    override func rightMouseDown(with event: NSEvent) {
        recorder.state.rightClicks += 1
        recorder.record("rightMouseDown")
    }

    override func mouseUp(with event: NSEvent) {
        recorder.state.mouseHeld = false
        recorder.state.mouseUps += 1
        recorder.record("mouseUp")
    }

    override func scrollWheel(with event: NSEvent) {
        recorder.state.scrollX += Double(event.scrollingDeltaX)
        recorder.state.scrollY += Double(event.scrollingDeltaY)
        recorder.record(
            "scroll", values: ["dx": event.scrollingDeltaX, "dy": event.scrollingDeltaY])
        needsDisplay = true
    }
}

final class FixtureDelegate: NSObject, NSApplicationDelegate, NSTextViewDelegate, NSWindowDelegate,
    NSOpenSavePanelDelegate
{
    let recorder: Recorder
    let window = NSWindow(
        contentRect: NSRect(x: 100, y: 180, width: 860, height: 680),
        styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered,
        defer: false)
    let clock = NSTextField(labelWithString: "UNDERSTUDY LIVE FIXTURE")
    private var inputMonitor: Any?

    init(recorder: Recorder) { self.recorder = recorder }

    func applicationDidFinishLaunching(_ notification: Notification) {
        configureMenu()
        inputMonitor = NSEvent.addLocalMonitorForEvents(
            matching: [.keyDown, .keyUp, .flagsChanged, .leftMouseDown, .leftMouseUp]
        ) { [weak self] event in
            guard let self else { return event }
            self.recorder.state.modifierFlags = UInt64(
                event.modifierFlags.intersection(.deviceIndependentFlagsMask).rawValue)
            self.recorder.record(
                "inputState",
                values: ["type": event.type.rawValue, "flags": self.recorder.state.modifierFlags])
            return event
        }
        window.title = "UNDERSTUDY CONTROL TEST • uid \(getuid())"
        window.acceptsMouseMovedEvents = true
        window.delegate = self
        guard let view = window.contentView else { return }
        clock.font = .monospacedSystemFont(ofSize: 24, weight: .bold)
        clock.frame = NSRect(x: 20, y: 620, width: 820, height: 40)
        view.addSubview(clock)
        let label = NSTextField(labelWithString: "Unicode text — type here; Cmd+A selects all")
        label.frame = NSRect(x: 20, y: 580, width: 780, height: 25)
        view.addSubview(label)
        let scroll = NSScrollView(frame: NSRect(x: 20, y: 350, width: 820, height: 220))
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        let text = NSTextView(frame: scroll.bounds)
        text.isRichText = false
        text.font = .systemFont(ofSize: 22)
        text.delegate = self
        text.isVerticallyResizable = true
        scroll.documentView = text
        view.addSubview(scroll)
        view.addSubview(
            PointerTarget(frame: NSRect(x: 20, y: 70, width: 820, height: 260), recorder: recorder))
        let dialog = NSButton(
            title: "Open Test Dialog", target: self, action: #selector(openDialog))
        dialog.frame = NSRect(x: 20, y: 20, width: 210, height: 35)
        view.addSubview(dialog)
        let save = NSButton(title: "Save Text…", target: self, action: #selector(saveText))
        save.frame = NSRect(x: 250, y: 20, width: 160, height: 35)
        view.addSubview(save)
        window.makeKeyAndOrderFront(nil)
        recordWindow("initialWindow")
        NSApp.activate(ignoringOtherApps: true)
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.recorder.state.ticks += 1
            self.clock.stringValue =
                "UNDERSTUDY uid \(getuid()) • tick \(self.recorder.state.ticks)"
            self.recorder.record("tick")
        }
        recorder.record("ready", values: ["pid": getpid()])
    }

    private func configureMenu() {
        let menu = NSMenu()
        func section(_ title: String) -> NSMenu {
            let item = NSMenuItem()
            menu.addItem(item)
            let submenu = NSMenu(title: title)
            item.submenu = submenu
            return submenu
        }
        section("Understudy").addItem(
            withTitle: "Quit Understudy Fixture", action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q")
        let save = section("File").addItem(
            withTitle: "Save…", action: #selector(saveText), keyEquivalent: "s")
        save.target = self
        let increment = section("Test").addItem(
            withTitle: "Increment Menu Counter", action: #selector(menuAction), keyEquivalent: "m")
        increment.target = self
        let edit = section("Edit")
        edit.addItem(
            withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        NSApp.mainMenu = menu
    }

    func textDidChange(_ notification: Notification) {
        guard let text = notification.object as? NSTextView else { return }
        recorder.state.text = text.string
        recorder.record("textChanged", values: ["length": text.string.count])
    }

    func textViewDidChangeSelection(_ notification: Notification) {
        guard let text = notification.object as? NSTextView else { return }
        recorder.state.selectionLength = text.selectedRange().length
        recorder.record("selectionChanged")
    }

    private func recordWindow(_ event: String) {
        recorder.state.windowX = window.frame.minX
        recorder.state.windowY = window.frame.minY
        recorder.state.windowWidth = window.frame.width
        recorder.state.windowHeight = window.frame.height
        recorder.record(event)
    }

    func windowDidMove(_ notification: Notification) {
        recordWindow("windowMoved")
    }

    func windowDidResize(_ notification: Notification) { recordWindow("windowResized") }

    @objc func menuAction() {
        recorder.state.menuCount += 1
        recorder.record("menu")
    }

    @objc func openDialog() {
        recorder.state.dialogCount += 1
        recorder.record("dialogOpen")
        let alert = NSAlert()
        alert.messageText = "Understudy disposable dialog"
        alert.informativeText = "Choose Confirm to record a verified response."
        alert.addButton(withTitle: "Confirm")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { response in
            self.recorder.state.dialogResponse = response.rawValue
            self.recorder.record("dialogClosed")
        }
    }

    @objc func saveText() {
        let panel = NSSavePanel()
        panel.directoryURL = recorder.directory
        panel.nameFieldStringValue = "understudy-fixture.txt"
        panel.delegate = self
        recorder.record(
            "saveOpened", values: ["directory": recorder.directory.path, "active": NSApp.isActive])
        panel.beginSheetModal(for: window) { response in
            if response == .OK, let url = panel.url {
                do {
                    try self.recorder.state.text.write(to: url, atomically: true, encoding: .utf8)
                    self.recorder.state.savedPath = url.path
                } catch { self.recorder.state.saveError = String(describing: error) }
            }
            self.recorder.record("saveClosed", values: ["response": response.rawValue])
        }
    }

    func panel(_ sender: Any, validate url: URL) throws {
        recorder.record("saveValidated", values: ["path": url.path])
    }
}

let app = NSApplication.shared
guard CommandLine.arguments.count == 2 else { fatalError("Pass a fresh output directory") }
let recorder = try Recorder(directory: URL(fileURLWithPath: CommandLine.arguments[1]))
let delegate = FixtureDelegate(recorder: recorder)
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
