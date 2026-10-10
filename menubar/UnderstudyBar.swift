import AppKit

// Understudy's menu bar companion. It renders only what the local viewer
// reports. The capability URL arrives on stdin, never in argv, and the app
// quits when stdin closes, so it never outlives `understudy menubar`.

// MARK: - Model

/// What the panel shows, decoded from the viewer's status report.
struct Snapshot {
    var mode = "unavailable"
    var reason = "Connecting to the helper"
    var account = "Helper account"
    var display = "Unknown"
    var input = "Unknown"
    var activity = "No recent actions"

    var controlling: Bool { mode == "controlling" }

    init() {}

    init(status: [String: Any]) {
        let overview = status["overview"] as? [String: Any] ?? [:]
        mode = overview["mode"] as? String ?? "unavailable"
        reason = overview["reason"] as? String ?? reason
        account = overview["account"] as? String ?? account
        if let display = overview["display"] as? [String: Any],
            let width = display["width"] as? Double, let height = display["height"] as? Double
        {
            self.display = "\(Int(width)) × \(Int(height))"
        }
        let session = status["session"] as? [String: Any]
        let service = session?["inputService"] as? [String: Any]
        if service?["ready"] as? Bool == true, let pid = service?["pid"] as? Int {
            input = "Ready · PID \(pid)"
        } else {
            input = service?["ok"] as? Bool == true ? "Not ready" : "Unavailable"
        }
        if let row = (overview["activity"] as? [[String: Any]])?.first,
            let operation = row["operation"] as? String
        {
            let who = row["actor"] as? String == "viewer" ? "You" : "Agent"
            let age = (row["ageSeconds"] as? Double).map(Snapshot.ago) ?? ""
            let failed = row["ok"] as? Bool == true ? "" : " · failed"
            activity = "\(who) · \(operation)\(failed) · \(age)"
        }
    }

    static func ago(_ seconds: Double) -> String {
        if seconds < 5 { return "just now" }
        if seconds < 90 { return "\(Int(seconds.rounded())) s ago" }
        if seconds < 5400 { return "\(Int((seconds / 60).rounded())) min ago" }
        return "\(Int((seconds / 3600).rounded())) h ago"
    }
}

/// A colour that follows the light or dark appearance it is drawn in.
func adaptive(_ light: Int, _ dark: Int) -> NSColor {
    func rgb(_ value: Int) -> NSColor {
        NSColor(
            srgbRed: CGFloat((value >> 16) & 0xff) / 255, green: CGFloat((value >> 8) & 0xff) / 255,
            blue: CGFloat(value & 0xff) / 255, alpha: 1)
    }
    return NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? rgb(dark) : rgb(light)
    }
}

/// Label and colours for each mode, matching the web viewer.
struct Tone {
    let label: String
    let dot: NSColor
    let text: NSColor
    let fill: NSColor

    static func of(_ mode: String) -> Tone {
        switch mode {
        case "observing":
            return Tone(
                label: "Observing", dot: adaptive(0x406348, 0x8fbf98),
                text: adaptive(0x406348, 0xa9d2b0), fill: adaptive(0xe8eee4, 0x26332a))
        case "controlling":
            return Tone(
                label: "Viewer in control", dot: adaptive(0xb1d0a2, 0xb1d0a2),
                text: adaptive(0xffffff, 0x1b1e19), fill: adaptive(0x252722, 0xe6e8e1))
        case "agent":
            return Tone(
                label: "Agent active", dot: adaptive(0x3f5a7a, 0x9fbbe0),
                text: adaptive(0x3f5a7a, 0xb8cde8), fill: adaptive(0xe7edf4, 0x253244))
        case "recovering":
            return Tone(
                label: "Needs recovery", dot: adaptive(0xc27b45, 0xe0a172),
                text: adaptive(0x8d542d, 0xe8b48c), fill: adaptive(0xf6efe6, 0x3a2c20))
        case "blocked":
            return Tone(
                label: "Not ready", dot: adaptive(0xc27b45, 0xe0a172),
                text: adaptive(0x8d542d, 0xe8b48c), fill: adaptive(0xf6efe6, 0x3a2c20))
        default:
            return Tone(
                label: "Unavailable", dot: adaptive(0xb4b8ac, 0x80847a),
                text: .secondaryLabelColor, fill: .quaternaryLabelColor)
        }
    }
}

// MARK: - Viewer API

/// The local viewer, reached only through its capability URL.
final class Viewer {
    let base: URL
    let origin: String
    let session: URLSession

    init?(_ text: String) {
        guard let url = URL(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
            url.scheme == "http", url.host == "127.0.0.1", let port = url.port,
            url.absoluteString.hasSuffix("/")
        else { return nil }
        base = url
        origin = "http://127.0.0.1:\(port)"
        let configuration = URLSessionConfiguration.ephemeral
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        configuration.timeoutIntervalForRequest = 8
        session = URLSession(configuration: configuration)
    }

    private func request(_ route: String, post: Bool = false) -> URLRequest {
        var request = URLRequest(url: base.appendingPathComponent(route))
        request.setValue(origin, forHTTPHeaderField: "Origin")
        if post {
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = Data("{}".utf8)
        }
        return request
    }

    func status(_ done: @escaping ([String: Any]?) -> Void) {
        session.dataTask(with: request("status.json")) { data, _, _ in
            let object = data.flatMap { try? JSONSerialization.jsonObject(with: $0) }
            DispatchQueue.main.async { done(object as? [String: Any]) }
        }.resume()
    }

    func frame(_ done: @escaping (NSImage?) -> Void) {
        session.dataTask(with: request("frame.png")) { data, response, _ in
            let ok = (response as? HTTPURLResponse)?.statusCode == 200
            let image = ok ? data.flatMap(NSImage.init(data:)) : nil
            DispatchQueue.main.async { done(image) }
        }.resume()
    }

    func revoke(_ done: @escaping (String?) -> Void) {
        session.dataTask(with: request("control/revoke", post: true)) { data, _, error in
            let object = data.flatMap { try? JSONSerialization.jsonObject(with: $0) }
            let reply = object as? [String: Any]
            let failure =
                reply?["ok"] as? Bool == true
                ? nil : (reply?["error"] as? String ?? error?.localizedDescription ?? "Failed")
            DispatchQueue.main.async { done(failure) }
        }.resume()
    }
}

// MARK: - Views

/// The mode pill: a rounded capsule with a dot and a label.
final class PillView: NSView {
    var tone = Tone.of("unavailable") {
        didSet {
            invalidateIntrinsicContentSize()
            needsDisplay = true
        }
    }
    private let font = NSFont.systemFont(ofSize: 11, weight: .medium)

    override var intrinsicContentSize: NSSize {
        let width = (tone.label as NSString).size(withAttributes: [.font: font]).width
        return NSSize(width: ceil(width) + 28, height: 22)
    }

    override func draw(_ dirtyRect: NSRect) {
        let shape = NSBezierPath(roundedRect: bounds, xRadius: 11, yRadius: 11)
        tone.fill.setFill()
        shape.fill()
        tone.dot.setFill()
        NSBezierPath(ovalIn: NSRect(x: 10, y: bounds.midY - 3.5, width: 7, height: 7)).fill()
        let text = NSAttributedString(
            string: tone.label, attributes: [.font: font, .foregroundColor: tone.text])
        text.draw(at: NSPoint(x: 22, y: bounds.midY - text.size().height / 2))
    }
}

/// The helper desktop thumbnail, letterboxed on a dark rounded stage.
final class ThumbnailView: NSView {
    var image: NSImage? { didSet { needsDisplay = true } }
    var placeholder = "Waiting for the helper desktop…" { didSet { needsDisplay = true } }

    override var intrinsicContentSize: NSSize { NSSize(width: 312, height: 176) }

    override func draw(_ dirtyRect: NSRect) {
        let stage = NSBezierPath(roundedRect: bounds, xRadius: 8, yRadius: 8)
        NSColor(srgbRed: 0.106, green: 0.118, blue: 0.098, alpha: 1).setFill()
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
                    .foregroundColor: NSColor(srgbRed: 0.77, green: 0.8, blue: 0.74, alpha: 1),
                ])
            let size = text.size()
            text.draw(
                at: NSPoint(
                    x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2))
        }
        NSGraphicsContext.restoreGraphicsState()
        NSColor.separatorColor.setStroke()
        stage.lineWidth = 1
        stage.stroke()
    }
}

func label(_ text: String, size: CGFloat = 12, color: NSColor = .labelColor) -> NSTextField {
    let field = NSTextField(labelWithString: text)
    field.font = .systemFont(ofSize: size)
    field.textColor = color
    field.lineBreakMode = .byTruncatingTail
    return field
}

// MARK: - Panel

/// The popover's content: state, desktop, facts and actions.
final class PanelController: NSViewController {
    let viewer: Viewer?
    let pill = PillView()
    let reason = label("", color: .secondaryLabelColor)
    let thumbnail = ThumbnailView()
    let account = label("")
    let display = label("")
    let input = label("")
    let activity = label("")
    let message = label("", size: 11, color: .secondaryLabelColor)
    let open = NSButton(title: "Open Viewer", target: nil, action: nil)
    let release = NSButton(title: "Release Control", target: nil, action: nil)
    let quit = NSButton(title: "Quit", target: nil, action: nil)

    init(viewer: Viewer?) {
        self.viewer = viewer
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    override func loadView() {
        let title = NSTextField(labelWithString: "Understudy")
        title.font =
            NSFont(name: "Iowan Old Style", size: 19) ?? .systemFont(ofSize: 17, weight: .semibold)
        let header = NSStackView(views: [title, NSView(), pill])
        header.distribution = .fill
        reason.maximumNumberOfLines = 2
        reason.lineBreakMode = .byWordWrapping
        reason.preferredMaxLayoutWidth = 312

        let rows = [
            ("Account", account), ("Display", display), ("Input", input), ("Last action", activity),
        ]
        let grid = NSGridView(
            views: rows.map { name, value in
                value.alignment = .right
                return [label(name, color: .secondaryLabelColor), value]
            })
        grid.rowSpacing = 6
        grid.column(at: 0).xPlacement = .leading
        grid.column(at: 1).xPlacement = .trailing

        open.bezelStyle = .rounded
        open.keyEquivalent = "\r"
        open.target = self
        open.action = #selector(openViewer)
        release.bezelStyle = .rounded
        release.target = self
        release.action = #selector(releaseControl)
        quit.bezelStyle = .rounded
        quit.target = NSApp
        quit.action = #selector(NSApplication.terminate(_:))
        let actions = NSStackView(views: [open, release, NSView(), quit])

        let separator = NSBox()
        separator.boxType = .separator
        let stack = NSStackView(views: [
            header, reason, thumbnail, grid, separator, actions, message,
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 12
        stack.setCustomSpacing(4, after: header)
        stack.edgeInsets = NSEdgeInsets(top: 16, left: 14, bottom: 14, right: 14)
        for view in [header, grid, separator, actions] {
            view.translatesAutoresizingMaskIntoConstraints = false
            view.widthAnchor.constraint(equalToConstant: 312).isActive = true
        }
        stack.widthAnchor.constraint(equalToConstant: 340).isActive = true
        message.isHidden = true
        view = stack
        show(Snapshot())
    }

    func show(_ snapshot: Snapshot) {
        pill.tone = Tone.of(snapshot.mode)
        reason.stringValue = snapshot.reason
        account.stringValue = snapshot.account
        display.stringValue = snapshot.display
        input.stringValue = snapshot.input
        activity.stringValue = snapshot.activity
        release.isEnabled = snapshot.controlling
    }

    @objc func openViewer() {
        if let url = viewer?.base { NSWorkspace.shared.open(url) }
    }

    @objc func releaseControl() {
        release.isEnabled = false
        viewer?.revoke { [weak self] failure in
            self?.message.stringValue = failure ?? "Control released. Nothing was resumed."
            self?.message.isHidden = false
        }
    }
}

// MARK: - Status item

/// The menu bar glyph: two stacked desktops and a dot in the mode's colour.
func statusImage(_ tone: Tone) -> NSImage {
    let image = NSImage(size: NSSize(width: 22, height: 16), flipped: false) { _ in
        NSColor.labelColor.setStroke()
        let back = NSBezierPath(
            roundedRect: NSRect(x: 5.5, y: 5.5, width: 13, height: 9), xRadius: 2, yRadius: 2)
        back.lineWidth = 1.2
        back.stroke()
        NSColor.labelColor.setFill()
        let front = NSBezierPath(
            roundedRect: NSRect(x: 2, y: 2, width: 13, height: 9), xRadius: 2, yRadius: 2)
        NSColor.windowBackgroundColor.setFill()
        front.fill()
        front.lineWidth = 1.2
        front.stroke()
        NSColor.windowBackgroundColor.setFill()
        NSBezierPath(ovalIn: NSRect(x: 13, y: 0, width: 9, height: 9)).fill()
        tone.dot.setFill()
        NSBezierPath(ovalIn: NSRect(x: 14.5, y: 1.5, width: 6, height: 6)).fill()
        return true
    }
    image.isTemplate = false
    return image
}

final class Companion: NSObject, NSApplicationDelegate, NSPopoverDelegate {
    let viewer: Viewer
    let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    let popover = NSPopover()
    let panel: PanelController
    var timer: Timer?
    // One request of each kind at a time, so a slow reply can never land
    // after, and overwrite, a newer one.
    var statusPending = false
    var framePending = false

    init(viewer: Viewer) {
        self.viewer = viewer
        panel = PanelController(viewer: viewer)
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        popover.contentViewController = panel
        popover.behavior = .transient
        popover.delegate = self
        item.button?.image = statusImage(Tone.of("unavailable"))
        item.button?.target = self
        item.button?.action = #selector(toggle)
        schedule(open: false)
    }

    @objc func toggle() {
        guard let button = item.button else { return }
        if popover.isShown {
            popover.performClose(nil)
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
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
                item.button?.image = statusImage(Tone.of(snapshot.mode))
                item.button?.toolTip = "Understudy — \(snapshot.reason)"
                panel.show(snapshot)
            }
        }
        if withFrame && !framePending {
            framePending = true
            viewer.frame { [weak self] image in
                guard let self else { return }
                framePending = false
                // A failed capture never leaves an old desktop on show.
                panel.thumbnail.placeholder = "The helper desktop is unavailable"
                panel.thumbnail.image = image
            }
        }
    }
}

// MARK: - Render mode

/// Draw the panel and its menu bar glyph, as a person would see them, to a
/// PNG. Used for visual checks without capturing the host screen.
func render(viewer: Viewer, to path: String, appearance name: NSAppearance.Name) {
    let panel = PanelController(viewer: viewer)
    var status: [String: Any]?
    var frame: NSImage?
    let group = DispatchGroup()
    group.enter()
    viewer.status {
        status = $0
        group.leave()
    }
    group.enter()
    viewer.frame {
        frame = $0
        group.leave()
    }
    group.notify(queue: .main) {
        let appearance = NSAppearance(named: name)!
        let snapshot = status.map(Snapshot.init(status:)) ?? Snapshot()
        let content = panel.view
        content.appearance = appearance
        panel.show(snapshot)
        panel.thumbnail.image = frame
        content.layoutSubtreeIfNeeded()
        let size = content.fittingSize
        let bar: CGFloat = 28
        let canvas = NSView(
            frame: NSRect(x: 0, y: 0, width: size.width, height: size.height + bar + 8))
        canvas.appearance = appearance
        let background = NSBox(frame: canvas.bounds)
        background.boxType = .custom
        background.borderWidth = 0
        background.fillColor = .windowBackgroundColor
        canvas.addSubview(background)
        let strip = NSBox(frame: NSRect(x: 0, y: size.height + 8, width: size.width, height: bar))
        strip.boxType = .custom
        strip.borderWidth = 0
        strip.fillColor = .underPageBackgroundColor
        canvas.addSubview(strip)
        let glyph = NSImageView(image: statusImage(Tone.of(snapshot.mode)))
        glyph.frame = NSRect(x: size.width - 40, y: size.height + 14, width: 22, height: 16)
        canvas.addSubview(glyph)
        content.frame = NSRect(origin: .zero, size: size)
        canvas.addSubview(content)
        canvas.layoutSubtreeIfNeeded()
        guard let bitmap = canvas.bitmapImageRepForCachingDisplay(in: canvas.bounds) else {
            exit(3)
        }
        appearance.performAsCurrentDrawingAppearance {
            canvas.cacheDisplay(in: canvas.bounds, to: bitmap)
        }
        guard let png = bitmap.representation(using: .png, properties: [:]),
            (try? png.write(to: URL(fileURLWithPath: path))) != nil
        else { exit(4) }
        exit(0)
    }
}

// MARK: - Entry

/// Quit when the parent closes stdin, so the bar never outlives the server.
func watchParent() {
    Thread.detachNewThread {
        while readLine() != nil {}
        DispatchQueue.main.async { NSApp.terminate(nil) }
    }
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
    let companion = Companion(viewer: viewer)
    app.delegate = companion
    watchParent()
    app.run()
}
