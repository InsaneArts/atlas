import AppKit
import AtlasKit
import Observation

/// The viewer window's state: the helper's status and latest picture, and
/// what this window is doing (watching, paused, holding control). Pictures
/// stay in memory only. Every effect is a request to the viewer service,
/// which owns the helper's safety.
@MainActor
@Observable
final class ViewerModel {
    private(set) var status = ViewerStatus()
    /// Whether any status has been read yet.
    private(set) var hasStatus = false
    /// The latest picture and what it was taken of.
    private(set) var frame: Frame?
    private(set) var image: NSImage?
    /// When the picture arrived, in seconds of uptime (never steps back).
    private(set) var frameAt: TimeInterval?
    /// Why the picture cannot be shown, when it cannot.
    private(set) var unavailable: String?
    /// The helper accepts input (its status said so with the last picture).
    private(set) var ready = false
    private(set) var paused = false
    private(set) var recovering = false
    /// One line about the last thing that happened, until the next.
    private(set) var notice: String?
    /// Locks the screen so the person can log in to the helper (set by the app).
    @ObservationIgnored var logIn: (@MainActor () -> Void)?

    /// Seconds between pictures while watching.
    var rate: Double = 2 {
        didSet {
            guard !still else { return }
            UserDefaults.standard.set(rate, forKey: Self.rateKey)
            refresh()
        }
    }
    var sidebarShown = false {
        didSet { if !still { UserDefaults.standard.set(sidebarShown, forKey: Self.sidebarKey) } }
    }

    // Control: the lease this window holds, and an acquisition under way.
    var lease: String?
    var acquiring = false
    /// Text the input method is composing, not yet sent.
    var composing = ""
    /// The desktop takes the keyboard (while in control).
    var keyboardConnected = false
    /// The window is in full screen (its title bar buttons are hidden).
    var fullScreen = false

    /// The recovery alert set aside with Not Now, until its reason changes.
    @ObservationIgnored private var recoveryLater: String?

    @ObservationIgnored let service: any ViewerService
    @ObservationIgnored private var active = false
    /// The window is on screen, and takes the keyboard.
    @ObservationIgnored var shown = false
    @ObservationIgnored var focused = true

    // Control's working parts (see ViewerModel+Control.swift).
    @ObservationIgnored var controlGeneration = 0
    @ObservationIgnored var heartbeat: Task<Void, Never>?
    /// One heartbeat tick (tests shorten it); the lease is renewed every
    /// `renewEvery` ticks.
    @ObservationIgnored var heartbeatInterval: Duration = .milliseconds(500)
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
    /// The saved setting keeps the sidebar's old name.
    static let sidebarKey = "viewer.inspector"
    static let rates: [Double] = [1, 2, 5]
    /// Seconds between pictures while holding control.
    static let controlRate = 0.5

    init(service: any ViewerService, still: Bool = false) {
        self.service = service
        self.still = still
        let saved = UserDefaults.standard.double(forKey: Self.rateKey)
        rate = Self.rates.contains(saved) ? saved : 2
        sidebarShown = UserDefaults.standard.bool(forKey: Self.sidebarKey)
        input.sink = self
    }

    // MARK: What the window shows

    var overview: Overview { Overview(status) }
    var holdingControl: Bool { lease != nil }
    var loggedOut: Bool { status.overview.mode == .loggedOut }

    var mode: Mode {
        if paused { return Mode(label: "Paused", tone: .quiet, mark: .dimmed) }
        let mode = Mode.seen(by: status.overview, holdingControl: holdingControl)
        if unavailable != nil && mode.tone != .attention && !loggedOut {
            return Mode(label: "Reconnecting", tone: .quiet, mark: .dimmed)
        }
        return hasStatus || unavailable != nil ? mode : Mode(label: "Connecting", tone: .quiet, mark: .dimmed)
    }

    /// The account name in the header.
    var account: String {
        status.overview.accountName ?? status.session.userName ?? status.overview.account ?? "Helper"
    }

    /// "Live", "Updated 4 s ago"…, read again each second by the header.
    func freshness() -> String {
        if paused { return "Picture paused" }
        if frame == nil && unavailable != nil { return "No picture" }
        return Words.freshness(frameAt.map { clock() - $0 })
    }

    /// The latest actions over the desktop, while watching with the sidebar closed.
    var captions: [ActivityItem] {
        holdingControl || paused || sidebarShown ? [] : overview.captions()
    }

    /// Why taking control must wait now, in words, if it must.
    var controlBlocked: String? {
        holdingControl ? nil : overview.controlBlocked(holdingControl: false)
    }

    var canTakeControl: Bool {
        !acquiring && !holdingControl && ready && !paused && frame != nil && controlBlocked == nil && !recovering
    }

    /// The recovery alert the status calls for now, set aside or not.
    private var recoveryOffered: Recovery? {
        overview.recovery(holdingControl: holdingControl, recovering: recovering)
    }

    /// The recovery alert, unless set aside with Not Now.
    var recovery: Recovery? {
        guard let offered = recoveryOffered, offered.busy || recoveryLater != laterKey(offered) else { return nil }
        return offered
    }

    /// Recovery was set aside: the footer offers it again.
    var recoveryDeferred: Bool {
        guard let offered = recoveryOffered else { return false }
        return !offered.busy && recoveryLater == laterKey(offered)
    }

    private func laterKey(_ recovery: Recovery) -> String { "\(status.overview.mode.rawValue):\(recovery.reason)" }

    /// The line beside the footer's buttons.
    var hint: String {
        if holdingControl {
            return keyboardConnected
                ? "Your mouse and keyboard go to the helper."
                : "Click the helper's screen to connect your keyboard."
        }
        if acquiring { return "Connecting…" }
        if paused { return "Picture paused. Work in the helper continues." }
        if let blocked = controlBlocked { return "\(blocked)." }
        if unavailable != nil { return "Control returns once the helper's screen is back." }
        if hasStatus && !ready && frame != nil { return "Mouse and keyboard aren't ready in the helper yet." }
        return "Watching. Control-Shift-Escape takes control."
    }

    // MARK: Reading

    /// Read while the window is on screen and not paused; stop otherwise.
    func setActive(_ active: Bool) {
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
    func refresh() {
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
            let seconds = holdingControl ? Self.controlRate : rate
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
                // A silent helper is explained in the app's words, not commands.
                if !reply.overview.mode.answers, reply.error == nil {
                    throw ServiceError(reply.overview.reason)
                }
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
                throw ServiceError("The helper's screen size is unknown.")
            }
            if let frame, frame.revision != next.revision {
                releaseControl("The helper changed. Take control again when ready.")
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

    /// What the panel last saw, shown while the first reading is under
    /// way. It is only a picture: control waits for a reading of its own.
    func begin(with status: ViewerStatus, image: NSImage?) {
        guard !hasStatus, frame == nil else { return }
        self.status = status
        hasStatus = status != ViewerStatus()
        self.image = image
    }

    /// Read the status once, outside the picture loop, and show it.
    func readStatus() async {
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
        if recoveryOffered == nil { recoveryLater = nil }
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

    func togglePause() {
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
    func visible(_ shown: Bool) {
        self.shown = shown
        if !shown { releaseControl() }
        setActive(shown && !paused)
    }

    func recoverLater() {
        if let offered = recoveryOffered { recoveryLater = laterKey(offered) }
    }

    func recoverNow() {
        recoveryLater = nil
    }

    /// Recover the helper. Never while holding control; never repeats an action.
    func recover() async {
        guard !recovering, !holdingControl else { return }
        recovering = true
        recoveryLater = nil
        do {
            let restarted = try await service.recover()
            say(restarted ? "The helper was recovered. Take control again when ready." : "The helper is ready again.")
        } catch {
            say(error.localizedDescription)
        }
        recovering = false
        // Read the outcome directly: a paused picture makes no requests.
        await readStatus()
        refresh()
    }

    /// The window is closing: give control back and stop reading.
    func close() {
        shown = false
        releaseControl()
        setActive(false)
    }

    // MARK: Drawing

    /// A viewer fixed at one state, for drawing it.
    static func still(
        _ status: ViewerStatus, image: NSImage?, display: ViewerStatus.Size = .init(width: 1512, height: 982),
        lease: String? = nil, paused: Bool = false, unavailable: String? = nil, sidebar: Bool = false,
        notice: String? = nil
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
        model.sidebarShown = sidebar
        model.notice = notice
        return model
    }
}

/// A service that is never asked anything (drawing only).
struct StillService: ViewerService {
    func status() async throws -> ViewerStatus { ViewerStatus() }
    func frame() async throws -> Frame { throw ServiceError("still") }
    func acquire(frame: String) async throws -> String { throw ServiceError("still") }
    func renew(lease: String, frame: String) async throws {}
    func release(lease: String) async throws {}
    func revoke() async throws {}
    func input(lease: String, frame: String, actions: [InputAction]) async throws {}
    func recover() async throws -> Bool { false }
}
