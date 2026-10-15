import AppKit
import AtlasKit
import Observation

/// The viewer window's state: the helper's status and latest picture, and
/// what this window is doing (watching, paused, holding control). Pictures
/// stay in memory only. Every effect is a request to the viewer service,
/// which owns the helper's safety.
@MainActor
@Observable
public final class ViewerModel {
    public enum Tab: String, CaseIterable, Sendable { case activity, session }

    public private(set) var status = ViewerStatus()
    /// Whether any status has been read yet.
    public private(set) var hasStatus = false
    /// The latest picture and what it was taken of.
    public private(set) var frame: Frame?
    public private(set) var image: NSImage?
    /// When the picture arrived, in seconds of uptime (never steps back).
    public private(set) var frameAt: TimeInterval?
    /// Why the picture cannot be shown, when it cannot.
    public private(set) var unavailable: String?
    /// The helper accepts input (its status said so with the last picture).
    public private(set) var ready = false
    public private(set) var paused = false
    public private(set) var recovering = false
    /// One line about the last thing that happened, until the next.
    public private(set) var notice: String?

    /// Seconds between pictures while watching.
    public var rate: Double = 2 {
        didSet {
            guard !still else { return }
            UserDefaults.standard.set(rate, forKey: Self.rateKey)
            refresh()
        }
    }
    public var inspectorShown = false {
        didSet { if !still { UserDefaults.standard.set(inspectorShown, forKey: Self.inspectorKey) } }
    }
    public var tab = Tab.activity

    // Control: the lease this window holds, and an acquisition under way.
    public internal(set) var lease: String?
    public internal(set) var acquiring = false
    /// Text the input method is composing, not yet sent.
    public internal(set) var composing = ""
    /// The desktop takes the keyboard (while in control).
    public internal(set) var keyboardConnected = false
    /// Readiness details shown open, when the person chose; else open while
    /// something is wrong.
    public var detailsChosen: Bool?
    /// The window is in full screen (its title bar buttons are hidden).
    public internal(set) var fullScreen = false

    /// The recovery alert set aside with Not Now, until its reason changes.
    @ObservationIgnored private var recoveryLater: String?

    @ObservationIgnored let service: any ViewerService
    @ObservationIgnored private var active = false
    /// The window is on screen, and takes the keyboard.
    @ObservationIgnored var shown = false
    @ObservationIgnored var focused = true

    // Control's working parts (see ViewerControl.swift).
    @ObservationIgnored var controlGeneration = 0
    @ObservationIgnored var heartbeat: Task<Void, Never>?
    @ObservationIgnored var gesture = PointerGesture()
    @ObservationIgnored var wheel = WheelAccumulator()
    @ObservationIgnored var wheelAt: DesktopPoint?
    @ObservationIgnored var wheelSending: Task<Void, Never>?
    @ObservationIgnored let input = InputQueue()
    /// The time now (tests set their own).
    @ObservationIgnored var clock: @MainActor () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var statusReading = 0
    @ObservationIgnored private var loop: Task<Void, Never>?
    @ObservationIgnored private var napping: Task<Void, Never>?
    @ObservationIgnored private var dirty = false
    /// Drawing only (render tests): never reads or sends anything.
    @ObservationIgnored let still: Bool

    static let rateKey = "viewer.rate"
    static let inspectorKey = "viewer.inspector"

    public init(service: any ViewerService, still: Bool = false) {
        self.service = service
        self.still = still
        let saved = UserDefaults.standard.double(forKey: Self.rateKey)
        rate = [1, 2, 5].contains(saved) ? saved : 2
        inspectorShown = UserDefaults.standard.bool(forKey: Self.inspectorKey)
        input.sink = self
    }

    // MARK: What the window shows

    var overview: Overview { Overview(status) }
    public var holdingControl: Bool { lease != nil }

    public var mode: Mode {
        if paused { return Mode(label: "Paused", tone: .quiet, mark: .dimmed) }
        let mode = Mode.seen(by: status.overview, holdingControl: holdingControl)
        if unavailable != nil && mode.tone != .attention {
            return Mode(label: "Reconnecting", tone: .quiet, mark: .dimmed)
        }
        return hasStatus || unavailable != nil ? mode : Mode(label: "Connecting", tone: .quiet, mark: .dimmed)
    }

    /// The account name in the header.
    public var account: String {
        status.overview.accountName ?? status.session.userName ?? status.overview.account ?? "Helper"
    }

    /// "Live", "Updated 4 s ago"…, read again each second by the header.
    public func freshness() -> String {
        if paused { return "Viewing paused" }
        if frame == nil && unavailable != nil { return "No current frame" }
        return Words.freshness(frameAt.map { clock() - $0 })
    }

    /// The latest actions over the desktop, while watching with the inspector closed.
    public var captions: [ActivityItem] {
        holdingControl || paused || inspectorShown ? [] : overview.captions()
    }

    /// Why taking control must wait now, in words, if it must.
    public var controlBlocked: String? {
        holdingControl ? nil : overview.controlBlocked(holdingControl: false)
    }

    public var canTakeControl: Bool {
        !acquiring && !holdingControl && ready && !paused && frame != nil && controlBlocked == nil && !recovering
    }

    /// The recovery alert, unless set aside with Not Now.
    public var recovery: Recovery? {
        guard let recovery = overview.recovery(holdingControl: holdingControl, recovering: recovering),
            recovery.busy || recoveryLater != laterKey(recovery)
        else { return nil }
        return recovery
    }

    /// Recovery was set aside: the dock offers it again.
    public var recoveryDeferred: Bool {
        guard let offered = overview.recovery(holdingControl: holdingControl, recovering: recovering) else {
            return false
        }
        return !offered.busy && recoveryLater == laterKey(offered)
    }

    private func laterKey(_ recovery: Recovery) -> String { "\(status.overview.mode):\(recovery.reason)" }

    /// The line under the dock.
    public var hint: String {
        if holdingControl {
            return keyboardConnected
                ? "Keyboard connected. Press Control-Shift-Esc to give control back."
                : "Click the desktop to connect your keyboard."
        }
        if acquiring { return "Connecting…" }
        if paused { return "Viewing paused. Work in the helper continues." }
        if let blocked = controlBlocked { return "\(blocked)." }
        if unavailable != nil { return "Control returns once the helper's screen is back." }
        if hasStatus && !ready && frame != nil { return "Mouse and keyboard aren't ready in the helper yet." }
        return "Choose Control to use your mouse and keyboard."
    }

    // MARK: Reading

    /// Read while the window is on screen and not paused; stop otherwise.
    public func setActive(_ active: Bool) {
        guard !still else { return }
        self.active = active
        generation += 1
        // One reading at a time across restarts too: the next loop starts
        // once the last one has ended.
        let previous = loop
        previous?.cancel()
        napping?.cancel()
        loop = nil
        if active {
            let number = generation
            loop = Task { [weak self] in
                await previous?.value
                _ = await self?.run(number)
            }
        }
    }

    /// Read again now, or right after the reading under way.
    public func refresh() {
        dirty = true
        napping?.cancel()
    }

    /// One reading at a time; each waits for the last to end.
    private func run(_ number: Int) async {
        while active && number == generation && !Task.isCancelled {
            dirty = false
            await poll(number)
            guard active, number == generation, !Task.isCancelled else { return }
            if dirty { continue }
            let seconds = holdingControl ? 0.5 : rate
            let nap = Task { _ = try? await Task.sleep(for: .seconds(seconds)) }
            napping = nap
            await nap.value
            napping = nil
        }
    }

    /// The status, then a picture. A reading overtaken by a newer one, or by
    /// a pause or close, has no effect at all, failures included.
    func poll(_ number: Int) async {
        statusReading += 1
        let reading = statusReading
        let current = { [unowned self] in active && number == generation && reading == statusReading }
        // With no picture yet, the first one is asked for alongside the
        // status, so the window shows the helper at once.
        async let early = frame == nil ? (try? await service.frame()) : nil
        do {
            let reply = try await service.status()
            guard current() else { return }
            show(reply)
            guard reply.canObserve else {
                let steps = reply.nextSteps.isEmpty ? [reply.error ?? "Helper unavailable"] : reply.nextSteps
                throw ServiceError(steps.joined(separator: " "))
            }
            let next: Frame
            if let early = await early {
                next = early
            } else {
                next = try await service.frame()
            }
            guard current() else { return }
            guard let picture = NSImage(data: next.png), picture.size.width > 0, picture.size.height > 0 else {
                throw ServiceError("Frame geometry is unavailable")
            }
            if let frame, frame.revision != next.revision {
                releaseControl("The helper changed. Choose Control again when ready.")
            }
            frame = next
            image = picture
            frameAt = clock()
            unavailable = nil
            ready = reply.ready
        } catch {
            if current() { lost(error) }
        }
    }

    /// What the menu bar last saw, shown while the first reading is under
    /// way. It is only a picture: control waits for a reading of its own.
    public func begin(with status: ViewerStatus, image: NSImage?) {
        guard !hasStatus, frame == nil else { return }
        self.status = status
        hasStatus = status != ViewerStatus()
        self.image = image
    }

    /// Read the status once, outside the picture loop, and show it.
    public func readStatus() async {
        statusReading += 1
        let reading = statusReading
        guard let reply = try? await service.status(), reading == statusReading else { return }
        show(reply)
    }

    private func show(_ reply: ViewerStatus) {
        status = reply
        hasStatus = true
        // Input not ready: control ends now, not with the next picture.
        if !reply.ready {
            ready = false
            if holdingControl || acquiring { releaseControl("Mouse and keyboard permission is unavailable.") }
        }
        // Not Now covers one incident: once nothing needs recovery, a later one alerts again.
        if overview.recovery(holdingControl: holdingControl, recovering: recovering) == nil { recoveryLater = nil }
    }

    /// No picture: say why, and give control back.
    private func lost(_ error: any Error) {
        ready = false
        frame = nil
        image = nil
        releaseControl()
        unavailable = error.localizedDescription
    }

    // MARK: Actions

    /// One line about what just happened; it stays until the next.
    func say(_ text: String?) {
        notice = text
    }

    /// Readiness details open: as the person chose, else while something is wrong.
    public var detailsShown: Bool {
        detailsChosen ?? (status.overview.mode == "blocked" || unavailable != nil)
    }

    public func togglePause() {
        paused.toggle()
        releaseControl()
        if !paused {
            ready = false
            frame = nil
            frameAt = nil
        }
        setActive(!paused && shown)
    }

    /// Resume reading after the window was hidden, unless paused.
    public func visible(_ shown: Bool) {
        self.shown = shown
        if !shown { releaseControl() }
        setActive(shown && !paused)
    }

    public func recoverLater() {
        if let offered = overview.recovery(holdingControl: holdingControl, recovering: recovering) {
            recoveryLater = laterKey(offered)
        }
    }

    public func recoverNow() {
        recoveryLater = nil
    }

    /// Recover the helper. Never while holding control; never repeats an action.
    public func recover() async {
        guard !recovering, !holdingControl else { return }
        recovering = true
        recoveryLater = nil
        do {
            let restarted = try await service.recover()
            say(
                restarted
                    ? "The helper was recovered. Choose Control again when ready."
                    : "The helper input service is ready.")
        } catch {
            say(error.localizedDescription)
        }
        recovering = false
        // Read the outcome directly: a paused picture makes no requests.
        await readStatus()
        refresh()
    }

    /// The window is closing: give control back and stop reading.
    public func close() {
        shown = false
        releaseControl()
        setActive(false)
    }

    // MARK: Drawing

    /// A viewer fixed at one state, for drawing it.
    public static func still(
        _ status: ViewerStatus, image: NSImage?, display: ViewerStatus.Size = .init(width: 1512, height: 982),
        lease: String? = nil, paused: Bool = false, unavailable: String? = nil, inspector: Bool = false,
        tab: Tab = .activity, notice: String? = nil
    ) -> ViewerModel {
        let model = ViewerModel(service: StillService(), still: true)
        model.status = status
        model.hasStatus = true
        model.image = image
        if image != nil {
            model.frame = Frame(png: Data(), id: "still", revision: "still", display: display)
            model.frameAt = model.clock()
        }
        model.ready = true
        model.lease = lease
        model.paused = paused
        model.unavailable = unavailable
        model.inspectorShown = inspector
        model.tab = tab
        model.notice = notice
        return model
    }
}

/// A service that is never asked anything (drawing only).
struct StillService: ViewerService {
    let base = URL(fileURLWithPath: "/")
    func status() async throws -> ViewerStatus { ViewerStatus() }
    func frame() async throws -> Frame { throw ServiceError("still") }
    func acquire(frame: String) async throws -> String { throw ServiceError("still") }
    func renew(lease: String, frame: String) async throws {}
    func release(lease: String) async throws {}
    func revoke() async throws {}
    func input(lease: String, frame: String, actions: [InputAction]) async throws {}
    func recover() async throws -> Bool { false }
}
