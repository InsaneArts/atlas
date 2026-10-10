import AppKit
import ApplicationServices
import CoreGraphics

struct Rectangle: Encodable {
    let x: Double
    let y: Double
    let width: Double
    let height: Double

    init(_ value: CGRect) {
        x = value.minX
        y = value.minY
        width = value.width
        height = value.height
    }
}

struct ScreenGeometry: Encodable {
    let displayID: UInt32
    let frame: Rectangle
    let visibleFrame: Rectangle
    let cgBounds: Rectangle
    let backingScale: Double

    init(_ screen: NSScreen) {
        displayID =
            (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?
            .uint32Value ?? 0
        frame = Rectangle(screen.frame)
        visibleFrame = Rectangle(screen.visibleFrame)
        cgBounds = Rectangle(CGDisplayBounds(displayID))
        backingScale = screen.backingScaleFactor
    }
}

struct PanelSnapshot: Encodable, Equatable {
    let attemptID: String
    let kind: String
    let active: Bool
    let keyWindow: Int?
    let mainWindow: Int?
    let firstResponder: String?
    let focusedRole: String?
    let focusedValue: String?
    let focusedSource: String?
    let focusAXTrusted: Bool
    let ownerWindowNumber: Int
    let ownerKey: Bool
    let ownerMain: Bool
    let windowNumber: Int
    let visible: Bool
    let key: Bool
    let attachedSheet: Int?
    let sheetParent: Int?
    let presented: Bool
    let directory: String?
    let filename: String?
    let selectedPaths: [String]
    let response: Int?
}

struct FixtureState: Encodable {
    let uid = getuid()
    let pid = getpid()
    let fixtureID = UUID().uuidString
    let panelInstrumentationVersion = 2
    var observerPID: Int32?
    var observerError: String?
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
    var titlebarHeight = 0.0
    var screen: ScreenGeometry?
    var centerCount = 0
    var dotX = 80.0
    var dotY = 100.0
    var scrollX = 0.0
    var scrollY = 0.0
    var menuCount = 0
    var dialogCount = 0
    var dialogResponse = 0
    var dialogAttemptID: String?
    var dialogCompletedAttemptID: String?
    var ticks = 0
    var text = ""
    var savedPath: String?
    var saveError: String?
    var openFixturePath: String?
    var openedPath: String?
    var openedText: String?
    var openError: String?
    var panel: PanelSnapshot?
}

struct ObservedControl: Encodable {
    let role: String?
    let title: String?
    let value: String?
    let enabled: Bool?
    let focused: Bool?
}

struct PanelObservation: Encodable {
    let uid = getuid()
    let observerPID = getpid()
    let parentPID: Int32
    let fixtureID: String
    let attemptID: String?
    let kind: String?
    let startedAt = Date().timeIntervalSince1970 * 1000
    var finishedAt = 0.0
    let trusted = AXIsProcessTrusted()
    let scanScope = "dialog-controls-without-file-list-contents"
    var scanComplete = false
    var ownerWindowPresent = false
    var panelExists = false
    var focusInPanel = false
    var focused: ObservedControl?
    var focusAncestors: [String] = []
    var omittedContentRoles: [String] = []
    var controls: [ObservedControl] = []
    var diagnostics: [String: Int32] = [:]
    var error: String?
}

// This child reads only its fixture parent's AX hierarchy. It never activates
// an application, requests consent, sets an AX attribute, or performs an action.
final class PanelObserver {
    let parentPID: Int32
    let fixtureID: String
    let directory: URL
    private let encoder = JSONEncoder()

    init(parentPID: Int32, fixtureID: String, directory: URL) throws {
        self.parentPID = parentPID
        self.fixtureID = fixtureID
        self.directory = directory.resolvingSymlinksInPath()
        let attributes = try FileManager.default.attributesOfItem(atPath: self.directory.path)
        guard parentPID > 1, getppid() == parentPID, getuid() == geteuid(),
            attributes[.type] as? FileAttributeType == .typeDirectory,
            attributes[.ownerAccountID] as? UInt32 == getuid(),
            let mode = attributes[.posixPermissions] as? UInt16, mode & 0o022 == 0
        else { throw CocoaError(.fileReadNoPermission) }
        var process = kinfo_proc()
        var size = MemoryLayout<kinfo_proc>.size
        var query = [CTL_KERN, KERN_PROC, KERN_PROC_PID, parentPID]
        guard sysctl(&query, UInt32(query.count), &process, &size, nil, 0) == 0,
            size == MemoryLayout<kinfo_proc>.size,
            process.kp_eproc.e_ucred.cr_uid == getuid()
        else { throw CocoaError(.fileReadNoPermission) }
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    }

    private func fixtureState() throws -> [String: Any] {
        let file = try FileHandle(forReadingFrom: directory.appendingPathComponent("state.json"))
        defer { try? file.close() }
        let data = try file.read(upToCount: 1_048_577) ?? Data()
        guard data.count <= 1_048_576,
            let state = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            state["pid"] as? Int32 == parentPID, state["uid"] as? UInt32 == getuid(),
            state["fixtureID"] as? String == fixtureID
        else { throw CocoaError(.fileReadCorruptFile) }
        return state
    }

    func run() throws {
        while getppid() == parentPID {
            let state = try fixtureState()
            let panel = state["panel"] as? [String: Any]
            var observation = PanelObservation(
                parentPID: parentPID, fixtureID: fixtureID,
                attemptID: panel?["attemptID"] as? String, kind: panel?["kind"] as? String)
            if observation.trusted, observation.attemptID != nil {
                inspect(&observation)
            }
            // A completed read may belong to an attempt that just ended or was
            // replaced. Never label those values as a newer attempt's evidence.
            let current = try fixtureState()["panel"] as? [String: Any]
            if current?["attemptID"] as? String != observation.attemptID { continue }
            guard getppid() == parentPID else { return }
            observation.finishedAt = Date().timeIntervalSince1970 * 1000
            let file = directory.appendingPathComponent("panel-observer.json")
            try encoder.encode(observation).write(to: file, options: .atomic)
            Thread.sleep(forTimeInterval: 0.1)
        }
    }

    private func inspect(_ observation: inout PanelObservation) {
        let deadline = ProcessInfo.processInfo.systemUptime + 1.5
        var failed = false
        func attribute(_ element: AXUIElement, _ name: String, _ stage: String) -> CFTypeRef? {
            guard ProcessInfo.processInfo.systemUptime < deadline else {
                observation.error = "AX observation exceeded its 1.5-second budget"
                failed = true
                return nil
            }
            let timeout = AXUIElementSetMessagingTimeout(element, 0.1)
            var value: CFTypeRef?
            let status =
                timeout == .success
                ? AXUIElementCopyAttributeValue(element, name as CFString, &value) : timeout
            if observation.diagnostics.count < 128 {
                observation.diagnostics[stage + "." + name] = status.rawValue
            }
            if status != .success, status != .noValue, status != .attributeUnsupported {
                failed = true
                if observation.error == nil {
                    observation.error = "\(stage).\(name) failed with AXError \(status.rawValue)"
                }
            }
            return status == .success ? value : nil
        }
        func element(_ value: CFTypeRef?) -> AXUIElement? {
            guard let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
            return (value as! AXUIElement)
        }
        func describe(_ value: AXUIElement, _ stage: String) -> ObservedControl {
            let role = attribute(value, "AXRole", stage) as? String
            let textRoles = ["AXTextField", "AXComboBox", "AXTextArea", "AXPopUpButton"]
            // AXValue on containers can be a large collection or an error.
            // The test only needs values from actual text/path controls.
            let text =
                textRoles.contains(role ?? "")
                ? attribute(value, "AXValue", stage) as? String : nil
            return ObservedControl(
                role: role,
                title: (attribute(value, "AXTitle", stage) as? String).map {
                    String($0.prefix(1024))
                },
                value: text.map { String($0.prefix(1024)) },
                enabled: attribute(value, "AXEnabled", stage) as? Bool,
                focused: attribute(value, "AXFocused", stage) as? Bool)
        }
        let application = AXUIElementCreateApplication(parentPID)
        let focused = element(attribute(application, "AXFocusedUIElement", "application"))
        if let focused { observation.focused = describe(focused, "focus") }
        var focusAncestors: [AXUIElement] = []
        var ancestor = focused
        while let current = ancestor, focusAncestors.count < 16 {
            if focusAncestors.contains(where: { CFEqual($0, current) }) { break }
            focusAncestors.append(current)
            let stage = "focusParent\(focusAncestors.count)"
            let role = attribute(current, "AXRole", stage) as? String
            observation.focusAncestors.append(role ?? "unknown")
            if role == "AXApplication" { break }
            ancestor = element(attribute(current, "AXParent", stage))
        }
        guard let windows = attribute(application, "AXWindows", "application") as? [AXUIElement]
        else { return }
        var queue = windows.map { (element: $0, depth: 0, inPanel: false) }
        var visited: [AXUIElement] = []
        var index = 0
        while index < queue.count, index < 256, ProcessInfo.processInfo.systemUptime < deadline {
            let entry = queue[index]
            let stage = "node\(index)"
            index += 1
            if visited.contains(where: { CFEqual($0, entry.element) }) { continue }
            visited.append(entry.element)
            let role = attribute(entry.element, "AXRole", stage) as? String
            let title = attribute(entry.element, "AXTitle", stage) as? String
            if role == "AXWindow", title == "UNDERSTUDY CONTROL TEST • uid \(getuid())" {
                observation.ownerWindowPresent = true
            }
            let panelWindow = role == "AXWindow" && title == observation.kind?.capitalized
            let inPanel = entry.inPanel || role == "AXSheet" || panelWindow
            if inPanel {
                observation.panelExists = true
                let control = describe(entry.element, stage)
                observation.controls.append(control)
                if focusAncestors.contains(where: { CFEqual($0, entry.element) }) {
                    observation.focusInPanel = true
                }
            }
            // File lists may expose hundreds of rows. They are not dialog
            // controls, and selected-file correctness is checked separately
            // using NSOpenPanel.urls and the bytes the fixture actually reads.
            if let role, ["AXBrowser", "AXOutline", "AXTable", "AXList"].contains(role) {
                if !observation.omittedContentRoles.contains(role) {
                    observation.omittedContentRoles.append(role)
                }
                continue
            }
            // The fixture owns one window. Its sheet is an immediate child;
            // there is no reason to inspect the rest of the fixture's controls.
            guard inPanel || entry.depth < 2 else { continue }
            guard let children = attribute(entry.element, "AXChildren", stage) as? [AXUIElement]
            else { continue }
            if children.count > 64 || (entry.depth >= 16 && !children.isEmpty) {
                failed = true
                observation.error = "AX hierarchy exceeded the fixture observation bounds"
            }
            if entry.depth < 16 {
                queue.append(
                    contentsOf: children.prefix(64).map { ($0, entry.depth + 1, inPanel) })
            }
        }
        observation.scanComplete = !failed && index == queue.count
        if index < queue.count { observation.error = "AX hierarchy was not completely observed" }
    }
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
            if let panel = state.panel {
                entry["panel"] = try JSONSerialization.jsonObject(with: encoder.encode(panel))
            }
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
    private var panelAttempt: (id: String, kind: String, panel: NSSavePanel)?
    private var panelResponse: Int?
    private var panelTimer: Timer?
    private var panelObserver: Process?

    init(recorder: Recorder) { self.recorder = recorder }

    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            let file = recorder.directory.appendingPathComponent("understudy-open-fixture.txt")
            let contents = Data("Understudy native Open — ქართული 日本語 🧑🏽‍💻 é\n".utf8)
            if FileManager.default.fileExists(atPath: file.path) {
                guard try Data(contentsOf: file) == contents else {
                    throw CocoaError(.fileWriteFileExists)
                }
            } else {
                try contents.write(to: file, options: .withoutOverwriting)
            }
            recorder.state.openFixturePath = file.path
        } catch {
            recorder.state.openError = "Could not prepare Open fixture: \(error)"
        }
        configureMenu()
        inputMonitor = NSEvent.addLocalMonitorForEvents(
            matching: [
                .keyDown, .keyUp, .flagsChanged, .leftMouseDown, .leftMouseUp,
                .leftMouseDragged, .mouseMoved,
            ]
        ) { [weak self] event in
            guard let self else { return event }
            self.recorder.state.modifierFlags = UInt64(
                event.modifierFlags.intersection(.deviceIndependentFlagsMask).rawValue)
            var values: [String: Any] = [
                "type": event.type.rawValue, "flags": self.recorder.state.modifierFlags,
            ]
            if [.leftMouseDown, .leftMouseUp, .leftMouseDragged, .mouseMoved].contains(event.type) {
                values["timestamp"] = event.timestamp
                values["eventNumber"] = event.eventNumber
                values["windowNumber"] = event.windowNumber
                values["deltaX"] = event.deltaX
                values["deltaY"] = event.deltaY
                values["locationInWindow"] = [
                    "x": event.locationInWindow.x, "y": event.locationInWindow.y,
                ]
                if let cg = event.cgEvent {
                    values["globalPosition"] = ["x": cg.location.x, "y": cg.location.y]
                    values["cgEventNumber"] = cg.getIntegerValueField(.mouseEventNumber)
                }
                // This is the frame observed by the local event monitor,
                // before returning the event to AppKit, not an action result.
                let frame = self.window.frame
                values["fixtureFrameAtMonitor"] = [
                    "x": frame.minX, "y": frame.minY,
                    "width": frame.width, "height": frame.height,
                ]
            }
            self.recorder.record("inputState", values: values)
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
        let open = NSButton(title: "Open File…", target: self, action: #selector(openText))
        open.frame = NSRect(x: 430, y: 20, width: 160, height: 35)
        view.addSubview(open)
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
        do {
            let observer = Process()
            observer.executableURL = URL(fileURLWithPath: CommandLine.arguments[0])
            observer.arguments = [
                "--observe-parent", String(getpid()), recorder.state.fixtureID,
                recorder.directory.path,
            ]
            try observer.run()
            panelObserver = observer
            recorder.state.observerPID = observer.processIdentifier
            recorder.record("observerStarted", values: ["pid": observer.processIdentifier])
        } catch {
            recorder.state.observerError = String(describing: error)
            recorder.record("observerFailed")
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        if panelObserver?.isRunning == true { panelObserver?.terminate() }
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
        let file = section("File")
        let save = file.addItem(
            withTitle: "Save…", action: #selector(saveText), keyEquivalent: "s")
        save.target = self
        let open = file.addItem(
            withTitle: "Open…", action: #selector(openText), keyEquivalent: "o")
        open.target = self
        let test = section("Test")
        let increment = test.addItem(
            withTitle: "Increment Menu Counter", action: #selector(menuAction), keyEquivalent: "m")
        increment.target = self
        let center = test.addItem(
            withTitle: "Center Window for Native Test", action: #selector(centerForNativeTest),
            keyEquivalent: "0")
        center.keyEquivalentModifierMask = [.command, .control]
        center.target = self
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
        recorder.state.titlebarHeight =
            window.frame.height - window.contentRect(forFrameRect: window.frame).height
        recorder.state.screen = window.screen.map(ScreenGeometry.init)
        recorder.record(event)
    }

    func windowDidMove(_ notification: Notification) {
        recordWindow("windowMoved")
    }

    func windowDidResize(_ notification: Notification) { recordWindow("windowResized") }

    func windowDidChangeScreen(_ notification: Notification) { recordWindow("screenChanged") }

    @objc func centerForNativeTest() {
        guard let screen = window.screen else { return }
        let visible = screen.visibleFrame
        window.setFrameOrigin(
            NSPoint(
                x: visible.midX - window.frame.width / 2,
                y: visible.midY - window.frame.height / 2))
        recorder.state.centerCount += 1
        recordWindow("centeredForNativeTest")
    }

    @objc func menuAction() {
        recorder.state.menuCount += 1
        recorder.record("menu")
    }

    @objc func openDialog() {
        guard window.attachedSheet == nil,
            recorder.state.dialogAttemptID == recorder.state.dialogCompletedAttemptID
        else {
            recorder.record("dialogBusy")
            return
        }
        let attemptID = UUID().uuidString
        recorder.state.dialogCount += 1
        recorder.state.dialogAttemptID = attemptID
        recorder.state.dialogCompletedAttemptID = nil
        recorder.state.dialogResponse = 0
        recorder.record("dialogOpen", values: ["attemptID": attemptID])
        let alert = NSAlert()
        alert.messageText = "Understudy disposable dialog"
        alert.informativeText = "Choose Confirm to record a verified response."
        alert.addButton(withTitle: "Confirm")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { response in
            guard self.recorder.state.dialogAttemptID == attemptID,
                self.recorder.state.dialogCompletedAttemptID == nil
            else {
                self.recorder.record(
                    "dialogStaleCompletion",
                    values: ["attemptID": attemptID, "response": response.rawValue])
                return
            }
            self.recorder.state.dialogCompletedAttemptID = attemptID
            self.recorder.state.dialogResponse = response.rawValue
            self.recorder.record(
                "dialogClosed", values: ["attemptID": attemptID, "response": response.rawValue])
        }
    }

    @objc func saveText() {
        let panel = NSSavePanel()
        panel.directoryURL = recorder.directory
        panel.isExtensionHidden = false
        panel.nameFieldStringValue = "understudy-fixture.txt"
        beginPanel(panel, kind: "save")
    }

    @objc func openText() {
        let panel = NSOpenPanel()
        panel.directoryURL = recorder.directory
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        beginPanel(panel, kind: "open")
    }

    private func beginPanel(_ panel: NSSavePanel, kind: String) {
        if panelAttempt != nil {
            snapshotPanel()
            guard panelResponse != nil, recorder.state.panel?.presented == false else {
                recorder.record("panelBusy")
                return
            }
        }
        let attemptID = UUID().uuidString
        panelAttempt = (attemptID, kind, panel)
        panelResponse = nil
        recorder.state.panel = nil
        if kind == "save" {
            recorder.state.savedPath = nil
            recorder.state.saveError = nil
        } else {
            recorder.state.openedPath = nil
            recorder.state.openedText = nil
            recorder.state.openError = nil
        }
        panel.delegate = self
        // This is a presentation request, not a readiness acknowledgement.
        recordPanelEvent(kind + "Opened", values: ["directory": recorder.directory.path])
        panel.beginSheetModal(for: window) { [self] response in
            guard panelAttempt?.id == attemptID else { return }
            panelResponse = response.rawValue
            if response == .OK, let url = panel.url {
                do {
                    if kind == "save" {
                        try recorder.state.text.write(to: url, atomically: true, encoding: .utf8)
                        recorder.state.savedPath = url.path
                    } else {
                        recorder.state.openedText = try String(contentsOf: url, encoding: .utf8)
                        recorder.state.openedPath = url.path
                    }
                } catch {
                    if kind == "save" {
                        recorder.state.saveError = String(describing: error)
                    } else {
                        recorder.state.openError = String(describing: error)
                    }
                }
            }
            snapshotPanel()
            recordPanelEvent(kind + "Closed", values: ["response": response.rawValue])
        }
        panelTimer?.invalidate()
        let timer = Timer(timeInterval: 0.1, repeats: true) { [weak self] _ in
            self?.snapshotPanel()
        }
        panelTimer = timer
        RunLoop.main.add(timer, forMode: .common)
        snapshotPanel()
    }

    private func snapshotPanel() {
        guard let attempt = panelAttempt else { return }
        let panel = attempt.panel
        let sheet = window.attachedSheet
        let focused = NSApp.accessibilityFocusedUIElement as? NSAccessibilityProtocol
        let localRole = focused?.accessibilityRole()?.rawValue
        let value = focused?.accessibilityValue() as? String
        let snapshot = PanelSnapshot(
            attemptID: attempt.id, kind: attempt.kind, active: NSApp.isActive,
            keyWindow: NSApp.keyWindow?.windowNumber,
            mainWindow: NSApp.mainWindow?.windowNumber,
            firstResponder: NSApp.keyWindow?.firstResponder.map {
                String(describing: type(of: $0))
            },
            focusedRole: localRole, focusedValue: value.map { String($0.prefix(1024)) },
            focusedSource: localRole != nil ? "appkit" : nil,
            focusAXTrusted: AXIsProcessTrusted(),
            ownerWindowNumber: window.windowNumber,
            ownerKey: window.isKeyWindow, ownerMain: window.isMainWindow,
            windowNumber: panel.windowNumber, visible: panel.isVisible, key: panel.isKeyWindow,
            attachedSheet: sheet?.windowNumber, sheetParent: panel.sheetParent?.windowNumber,
            presented: panel.isVisible || panel.sheetParent != nil || sheet != nil,
            directory: panel.directoryURL?.resolvingSymlinksInPath().path,
            filename: attempt.kind == "save" ? panel.nameFieldStringValue : nil,
            selectedPaths: (panel as? NSOpenPanel)?.urls.map { $0.resolvingSymlinksInPath().path }
                ?? [],
            response: panelResponse)
        if snapshot != recorder.state.panel {
            recorder.state.panel = snapshot
            recordPanelEvent("panelState")
        }
        if panelResponse != nil, !snapshot.presented { panelTimer?.invalidate() }
    }

    private func recordPanelEvent(_ event: String, values: [String: Any] = [:]) {
        guard let attempt = panelAttempt else { return }
        var values = values
        values["attemptID"] = attempt.id
        values["kind"] = attempt.kind
        values["active"] = NSApp.isActive
        recorder.record(event, values: values)
    }

    func panel(_ sender: Any, validate url: URL) throws {
        guard let attempt = panelAttempt, sender as? NSSavePanel === attempt.panel else { return }
        recordPanelEvent(attempt.kind + "Validated", values: ["path": url.path])
    }

    func panel(_ sender: Any, didChangeToDirectoryURL url: URL?) {
        guard let attempt = panelAttempt, sender as? NSSavePanel === attempt.panel else { return }
        recordPanelEvent("panelDirectoryChanged", values: ["path": url?.path ?? NSNull()])
        snapshotPanel()
    }

    func panelSelectionDidChange(_ sender: Any?) {
        guard let attempt = panelAttempt, sender as? NSSavePanel === attempt.panel else { return }
        recordPanelEvent("panelSelectionChanged")
        snapshotPanel()
    }
}

if CommandLine.arguments.count == 5, CommandLine.arguments[1] == "--observe-parent",
    let parentPID = Int32(CommandLine.arguments[2])
{
    do {
        try PanelObserver(
            parentPID: parentPID, fixtureID: CommandLine.arguments[3],
            directory: URL(fileURLWithPath: CommandLine.arguments[4])
        ).run()
        exit(0)
    } catch {
        fputs("Fixture panel observer failed: \(error)\n", stderr)
        exit(1)
    }
}

guard CommandLine.arguments.count == 2 else { fatalError("Pass a fresh output directory") }
let app = NSApplication.shared
let recorder = try Recorder(directory: URL(fileURLWithPath: CommandLine.arguments[1]))
let delegate = FixtureDelegate(recorder: recorder)
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
