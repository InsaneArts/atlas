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
    var display = ""
    var last = "No recent actions"
    var when = ""

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
        if let row = (overview["activity"] as? [[String: Any]])?.first,
            let operation = row["operation"] as? String
        {
            let who = row["actor"] as? String == "viewer" ? "You" : "Agent"
            last = "\(who) \(Snapshot.describe(operation, row))"
            when = (row["ageSeconds"] as? Double).map(Snapshot.ago) ?? ""
        }
    }

    /// What an operation did, in words; no payload is ever recorded.
    static func describe(_ operation: String, _ row: [String: Any]) -> String {
        let count = row["actionCount"] as? Int ?? 0
        if row["ok"] as? Bool != true { return "\(operation) failed" }
        if operation == "input" && count > 0 {
            return "sent \(count) action\(count == 1 ? "" : "s")"
        }
        if operation == "run" { return "ran a command" }
        if operation == "restart" { return "recovered" }
        return operation
    }

    static func ago(_ seconds: Double) -> String {
        if seconds < 5 { return "Now" }
        if seconds < 90 { return "\(Int(seconds.rounded())) s ago" }
        if seconds < 5400 { return "\(Int((seconds / 60).rounded())) min ago" }
        return "\(Int((seconds / 3600).rounded())) h ago"
    }
}

/// How each mode reads: a label, one small dot, and the glyph's mark.
struct Mode {
    enum Mark { case none, working, filled, alert, dash, dimmed }

    let label: String
    let dot: NSColor
    let mark: Mark

    static func of(_ mode: String) -> Mode {
        switch mode {
        case "observing": return Mode(label: "Observing", dot: .systemGreen, mark: .none)
        case "agent": return Mode(label: "Agent working", dot: .controlAccentColor, mark: .working)
        case "controlling":
            return Mode(label: "Viewer has control", dot: .controlAccentColor, mark: .filled)
        case "recovering": return Mode(label: "Needs recovery", dot: .systemOrange, mark: .alert)
        case "blocked": return Mode(label: "Not ready", dot: .systemOrange, mark: .dash)
        default: return Mode(label: "Unavailable", dot: .tertiaryLabelColor, mark: .dimmed)
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

/// A small filled circle in the mode's colour.
final class DotView: NSView {
    var color = NSColor.tertiaryLabelColor { didSet { needsDisplay = true } }

    override var intrinsicContentSize: NSSize { NSSize(width: 6, height: 6) }

    override func draw(_ dirtyRect: NSRect) {
        color.setFill()
        NSBezierPath(ovalIn: bounds).fill()
    }
}

/// The helper desktop, letterboxed on a black rounded stage.
final class ThumbnailView: NSView {
    var image: NSImage? { didSet { needsDisplay = true } }
    var placeholder = "Waiting for the helper desktop…" { didSet { needsDisplay = true } }

    override var intrinsicContentSize: NSSize { NSSize(width: 316, height: 178) }

    override func draw(_ dirtyRect: NSRect) {
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
            let text = NSAttributedString(
                string: placeholder,
                attributes: [
                    .font: NSFont.systemFont(ofSize: 11),
                    .foregroundColor: NSColor(white: 0.65, alpha: 1),
                ])
            let size = text.size()
            text.draw(
                at: NSPoint(
                    x: (bounds.width - size.width) / 2, y: (bounds.height - size.height) / 2))
        }
        NSGraphicsContext.restoreGraphicsState()
    }
}

/// A grouped block of rows on a rounded background, as in System Settings.
final class GroupView: NSView {
    override var wantsUpdateLayer: Bool { false }

    override func draw(_ dirtyRect: NSRect) {
        let dark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        (dark ? NSColor(white: 1, alpha: 0.07) : NSColor.white).setFill()
        NSBezierPath(roundedRect: bounds, xRadius: 10, yRadius: 10).fill()
    }
}

func label(_ text: String, size: CGFloat = 13, color: NSColor = .labelColor) -> NSTextField {
    let field = NSTextField(labelWithString: text)
    field.font = .systemFont(ofSize: size)
    field.textColor = color
    field.lineBreakMode = .byTruncatingTail
    return field
}

/// One row of a group: a label on the left, a secondary value on the right.
func row(_ title: NSTextField, _ value: NSTextField) -> NSStackView {
    value.textColor = .secondaryLabelColor
    value.alignment = .right
    value.setContentCompressionResistancePriority(.required, for: .horizontal)
    title.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    let line = NSStackView(views: [title, NSView(), value])
    line.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
    return line
}

// MARK: - Panel

/// The popover's content: status, desktop, facts and actions.
final class PanelController: NSViewController {
    let viewer: Viewer?
    let dot = DotView()
    let state = label("", size: 12, color: .secondaryLabelColor)
    let thumbnail = ThumbnailView()
    let account = label("")
    let display = label("")
    let last = label("")
    let when = label("")
    let message = label("", size: 11, color: .secondaryLabelColor)
    let open = NSButton(title: "Open Viewer", target: nil, action: nil)
    let release = NSButton(title: "Release Control", target: nil, action: nil)

    init(viewer: Viewer?) {
        self.viewer = viewer
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    override func loadView() {
        let title = label("Understudy")
        title.font = .systemFont(ofSize: 13, weight: .semibold)
        let status = NSStackView(views: [dot, state])
        status.spacing = 6
        let header = NSStackView(views: [title, NSView(), status])
        header.edgeInsets = NSEdgeInsets(top: 0, left: 2, bottom: 0, right: 2)

        let separator = NSBox()
        separator.boxType = .separator
        let facts = NSStackView(views: [row(account, display), separator, row(last, when)])
        facts.orientation = .vertical
        facts.spacing = 0
        let group = GroupView()
        group.addSubview(facts)
        facts.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            facts.leadingAnchor.constraint(equalTo: group.leadingAnchor),
            facts.trailingAnchor.constraint(equalTo: group.trailingAnchor),
            facts.topAnchor.constraint(equalTo: group.topAnchor),
            facts.bottomAnchor.constraint(equalTo: group.bottomAnchor),
            separator.leadingAnchor.constraint(equalTo: facts.leadingAnchor, constant: 12),
            separator.trailingAnchor.constraint(equalTo: facts.trailingAnchor, constant: -12),
        ])

        for button in [open, release] {
            button.bezelStyle = .push
            button.controlSize = .large
            button.target = self
        }
        open.keyEquivalent = "\r"
        open.action = #selector(openViewer)
        release.action = #selector(releaseControl)
        let actions = NSStackView(views: [open, release])
        actions.orientation = .vertical
        actions.spacing = 6

        let divider = NSBox()
        divider.boxType = .separator
        let quit = NSButton(
            title: "Quit Understudy", target: NSApp, action: #selector(NSApplication.terminate(_:)))
        quit.isBordered = false
        quit.alignment = .left
        quit.keyEquivalent = "q"
        quit.keyEquivalentModifierMask = .command
        let shortcut = label("⌘Q", color: .secondaryLabelColor)
        let quitRow = NSStackView(views: [quit, NSView(), shortcut])
        quitRow.edgeInsets = NSEdgeInsets(top: 0, left: 8, bottom: 0, right: 8)

        message.isHidden = true
        let stack = NSStackView(views: [
            header, thumbnail, group, actions, message, divider, quitRow,
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 10
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 10, right: 12)
        for view in [header, thumbnail, group, actions, divider, quitRow, open, release] {
            view.translatesAutoresizingMaskIntoConstraints = false
            view.widthAnchor.constraint(equalToConstant: 316).isActive = true
        }
        thumbnail.heightAnchor.constraint(equalTo: thumbnail.widthAnchor, multiplier: 9.0 / 16.0)
            .isActive = true
        stack.widthAnchor.constraint(equalToConstant: 340).isActive = true
        view = stack
        show(Snapshot())
    }

    func show(_ snapshot: Snapshot) {
        let mode = Mode.of(snapshot.mode)
        dot.color = mode.dot
        state.stringValue = mode.label
        account.stringValue = snapshot.account
        display.stringValue = snapshot.display
        last.stringValue = snapshot.last
        when.stringValue = snapshot.when
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

/// The menu bar glyph: a monochrome template image of two stacked windows.
/// The shape, not a colour, carries the state, like the system's own items.
func statusImage(_ mode: Mode) -> NSImage {
    let image = NSImage(size: NSSize(width: 20, height: 15), flipped: true) { _ in
        let ink = NSColor.black.withAlphaComponent(mode.mark == .dimmed ? 0.4 : 1)
        let back = NSBezierPath(
            roundedRect: NSRect(x: 5.5, y: 0.9, width: 13.6, height: 9.4), xRadius: 2.2,
            yRadius: 2.2)
        let front = NSBezierPath(
            roundedRect: NSRect(x: 0.9, y: 4.7, width: 13.6, height: 9.4), xRadius: 2.2,
            yRadius: 2.2)
        ink.setStroke()
        back.lineWidth = 1.3
        back.stroke()
        // Clear the back window behind the front one, then draw the front.
        NSGraphicsContext.current?.compositingOperation = .clear
        front.fill()
        NSGraphicsContext.current?.compositingOperation = .sourceOver
        ink.set()
        front.lineWidth = 1.3
        if mode.mark == .filled { front.fill() } else { front.stroke() }
        switch mode.mark {
        case .working:
            for x in [4.6, 7.7, 10.8] {
                NSBezierPath(ovalIn: NSRect(x: x - 0.8, y: 8.6, width: 1.6, height: 1.6)).fill()
            }
        case .alert:
            let line = NSBezierPath()
            line.move(to: NSPoint(x: 7.7, y: 7.2))
            line.line(to: NSPoint(x: 7.7, y: 9.6))
            line.lineWidth = 1.4
            line.lineCapStyle = .round
            line.stroke()
            NSBezierPath(ovalIn: NSRect(x: 6.95, y: 10.9, width: 1.5, height: 1.5)).fill()
        case .dash:
            let line = NSBezierPath()
            line.move(to: NSPoint(x: 5.6, y: 9.4))
            line.line(to: NSPoint(x: 9.8, y: 9.4))
            line.lineWidth = 1.4
            line.lineCapStyle = .round
            line.stroke()
        default:
            break
        }
        return true
    }
    image.isTemplate = true
    image.accessibilityDescription = "Understudy: \(mode.label)"
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
        item.button?.image = statusImage(Mode.of("unavailable"))
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
                item.button?.image = statusImage(Mode.of(snapshot.mode))
                item.button?.toolTip = "Understudy — \(Mode.of(snapshot.mode).label)"
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
        let glyph = NSImageView(image: statusImage(Mode.of(snapshot.mode)))
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
