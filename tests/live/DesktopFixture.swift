// Native AppKit fixture for fresh live tests; historical live/ stays unchanged.
import AppKit
import CoreGraphics

let app = NSApplication.shared
guard CommandLine.arguments.count == 2 else { fatalError("Pass a fresh output directory") }
let logDir = CommandLine.arguments[1]
try! FileManager.default.createDirectory(atPath: logDir, withIntermediateDirectories: true)
var state: [String: Any] = ["clicks": 0, "doubleClicks": 0, "dragEvents": 0, "scrollX": 0.0, "scrollY": 0.0, "menuCount": 0, "dialogCount": 0, "text": "", "rightClicks": 0, "moves": 0, "ticks": 0, "uid": Int(getuid())]
func record(_ kind: String, _ values: [String: Any] = [:]) {
    var entry = values
    entry["event"] = kind
    entry["time"] = ISO8601DateFormatter().string(from: Date())
    entry["uptime"] = ProcessInfo.processInfo.systemUptime
    entry["sessionOnConsole"] = (CGSessionCopyCurrentDictionary() as? [String: Any])?["kCGSSessionOnConsoleKey"] ?? NSNull()
    let data = try! JSONSerialization.data(withJSONObject: entry, options: [.sortedKeys])
    let path = logDir + "/events.jsonl"
    if !FileManager.default.fileExists(atPath: path) { FileManager.default.createFile(atPath: path, contents: nil) }
    if let f = FileHandle(forWritingAtPath: path) { f.seekToEndOfFile(); f.write(data); f.write(Data([10])); try? f.close() }
    try? JSONSerialization.data(withJSONObject: state, options: [.prettyPrinted,.sortedKeys]).write(to: URL(fileURLWithPath: logDir + "/state.json"), options: .atomic)
    try? FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: logDir + "/state.json")
}
class Target: NSView {
    var dot = NSPoint(x: 80, y: 100)
    override var isFlipped: Bool { true }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .activeAlways, .inVisibleRect], owner: self))
    }
    override func mouseMoved(with e: NSEvent) { state["moves"] = (state["moves"] as! Int)+1; record("mouseMoved") }
    override func draw(_ rect: NSRect) {
        NSColor(calibratedWhite: 0.92, alpha: 1).setFill(); bounds.fill()
        ("CLICK / DOUBLE CLICK / DRAG / SCROLL" as NSString).draw(at: NSPoint(x: 15,y: 15), withAttributes: [.font:NSFont.systemFont(ofSize: 18)])
        NSColor.systemOrange.setFill(); NSBezierPath(ovalIn: NSRect(x: dot.x-20,y: dot.y-20,width:40,height:40)).fill()
        let summary = "Clicks: \(state["clicks"]!)   Doubles: \(state["doubleClicks"]!)   Drags: \(state["dragEvents"]!)\nScroll X: \(state["scrollX"]!)   Y: \(state["scrollY"]!)"
        (summary as NSString).draw(at:NSPoint(x:15,y:180),withAttributes:[.font:NSFont.monospacedSystemFont(ofSize:16,weight:.regular)])
    }
    override func mouseDown(with e: NSEvent) {
        state["clicks"] = (state["clicks"] as! Int)+1
        if e.clickCount == 2 { state["doubleClicks"] = (state["doubleClicks"] as! Int)+1 }
        record("mouseDown", ["clickCount":e.clickCount,"x":e.locationInWindow.x,"y":e.locationInWindow.y]); needsDisplay = true
    }
    override func mouseDragged(with e: NSEvent) {
        dot = convert(e.locationInWindow, from:nil); state["dragEvents"] = (state["dragEvents"] as! Int)+1
        state["dotX"] = dot.x; state["dotY"] = dot.y
        record("drag",["x":dot.x,"y":dot.y]); needsDisplay = true
    }
    override func rightMouseDown(with e: NSEvent) { state["rightClicks"] = (state["rightClicks"] as! Int)+1; record("rightMouseDown") }
    override func mouseUp(with e: NSEvent) { record("mouseUp") }
    override func scrollWheel(with e: NSEvent) {
        state["scrollX"] = (state["scrollX"] as! Double) + Double(e.scrollingDeltaX)
        state["scrollY"] = (state["scrollY"] as! Double) + Double(e.scrollingDeltaY)
        record("scroll",["dx":e.scrollingDeltaX,"dy":e.scrollingDeltaY]); needsDisplay = true
    }
}
class Delegate: NSObject, NSApplicationDelegate, NSTextViewDelegate {
    var window: NSWindow!
    var clock: NSTextField!
    func applicationDidFinishLaunching(_ n: Notification) {
        let main = NSMenu(); let appItem = NSMenuItem(); main.addItem(appItem)
        let appMenu = NSMenu(); appMenu.addItem(withTitle:"Quit Atlas Fixture",action:#selector(NSApplication.terminate(_:)),keyEquivalent:"q"); appItem.submenu = appMenu
        let testItem = NSMenuItem(); main.addItem(testItem); let testMenu = NSMenu(title:"Test"); testItem.submenu = testMenu
        let increment = testMenu.addItem(withTitle:"Increment Menu Counter",action:#selector(menuAction),keyEquivalent:"m"); increment.target = self
        let editItem = NSMenuItem(); main.addItem(editItem)
        let editMenu = NSMenu(title: "Edit"); editItem.submenu = editMenu
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        app.mainMenu = main
        window = NSWindow(contentRect:NSRect(x:100,y:180,width:860,height:680),styleMask:[.titled,.closable,.miniaturizable,.resizable],backing:.buffered,defer:false)
        window.acceptsMouseMovedEvents = true
        window.title = "UNDERSTUDY CONTROL TEST • uid \(getuid())"
        let view = window.contentView!
        clock = NSTextField(labelWithString:"UNDERSTUDY LIVE FIXTURE"); clock.font = .monospacedSystemFont(ofSize:24,weight:.bold); clock.frame = NSRect(x:20,y:620,width:820,height:40); view.addSubview(clock)
        let label = NSTextField(labelWithString:"Unicode text — type here; Cmd+A selects all"); label.frame = NSRect(x:20,y:580,width:780,height:25); view.addSubview(label)
        let scroll = NSScrollView(frame:NSRect(x:20,y:350,width:820,height:220)); scroll.hasVerticalScroller = true; scroll.borderType = .bezelBorder
        let text = NSTextView(frame:scroll.bounds); text.isRichText = false; text.font = .systemFont(ofSize:22); text.delegate = self; text.isVerticallyResizable = true; scroll.documentView = text; view.addSubview(scroll)
        let target = Target(frame:NSRect(x:20,y:70,width:820,height:260)); view.addSubview(target)
        let button = NSButton(title:"Open Test Dialog",target:self,action:#selector(dialog)); button.frame = NSRect(x:20,y:20,width:210,height:35); view.addSubview(button)
        window.makeKeyAndOrderFront(nil); app.activate(ignoringOtherApps:true)
        Timer.scheduledTimer(withTimeInterval:1,repeats:true) { _ in
            state["ticks"] = (state["ticks"] as! Int)+1
            self.clock.stringValue = "UNDERSTUDY uid \(getuid()) • tick \(state["ticks"]!)"
            record("tick")
        }
        record("ready",["pid":getpid()])
    }
    func textDidChange(_ n: Notification) { state["text"] = (n.object as! NSTextView).string; record("textChanged",["length":(state["text"] as! String).count]) }
    @objc func menuAction() { state["menuCount"] = (state["menuCount"] as! Int)+1; record("menu") }
    @objc func dialog() {
        state["dialogCount"] = (state["dialogCount"] as! Int)+1; record("dialogOpen")
        let alert = NSAlert(); alert.messageText = "Atlas disposable dialog"; alert.informativeText = "Choose Confirm to record a verified response."; alert.addButton(withTitle:"Confirm"); alert.addButton(withTitle:"Cancel")
        alert.beginSheetModal(for:window) { response in state["dialogResponse"] = response.rawValue; record("dialogClosed") }
    }
}
let delegate = Delegate(); app.delegate = delegate; app.setActivationPolicy(.regular); app.run()
