import AppKit
import AtlasKit
import Observation

/// What the menu bar panel shows: the latest status, the helper's desktop
/// and the last action's outcome. Every action is an `atlas` command or a
/// request to the viewer service; the panel holds no policy of its own.
@MainActor
@Observable
public final class PanelModel {
    public private(set) var status = ViewerStatus()
    public private(set) var image: NSImage?
    public private(set) var placeholder = "Waiting for the helper’s screen…"
    /// What the last action said, and whether it still runs.
    public private(set) var message: String?
    public private(set) var working = false
    /// The helper's short name, once `atlas switch --list` reports it.
    public private(set) var helper: String?
    public private(set) var atLogin = false
    public private(set) var loginAvailable = false

    /// Opens the setup window (set by the app).
    @ObservationIgnored public var gettingStarted: (@MainActor () -> Void)?
    /// Opens the viewer window (set by the app).
    @ObservationIgnored public var openViewer: (@MainActor () -> Void)?

    @ObservationIgnored let service: (any ViewerService)?
    @ObservationIgnored let commands: (any CommandRunning)?
    @ObservationIgnored let loginItem: LoginItem?
    @ObservationIgnored private var polling: [Task<Void, Never>] = []
    /// Number each reading, so only the newest one started is shown.
    @ObservationIgnored private var statusReading = 0
    @ObservationIgnored private var frameReading = 0

    public init(service: (any ViewerService)?, commands: (any CommandRunning)?, loginItem: LoginItem?) {
        self.service = service
        self.commands = commands
        self.loginItem = loginItem
        atLogin = loginItem?.enabled == true
        loginAvailable = loginItem?.available == true
    }

    public var mode: Mode { Mode.of(status.overview.mode) }
    var overview: Overview { Overview(status) }
    var controlling: Bool { status.overview.mode == "controlling" }
    var recovering: Bool { status.overview.mode == "recovering" }

    // MARK: Reading

    /// Read the status now and then every 2 s open or 5 s closed, and the
    /// desktop every 2 s while open, until called again or `stop()`. The two
    /// run apart, so a slow capture never holds the status back.
    public func watch(open: Bool) {
        stop()
        polling.append(repeating(every: open ? 2 : 5) { await $0.readStatus() })
        if open { polling.append(repeating(every: 2) { await $0.readFrame() }) }
    }

    public func stop() {
        polling.forEach { $0.cancel() }
        polling = []
    }

    /// Run `step` now and then every `seconds`, until cancelled or the model is gone.
    private func repeating(
        every seconds: Double, _ step: @escaping @MainActor (PanelModel) async -> Void
    )
        -> Task<Void, Never>
    {
        Task { [weak self] in
            while !Task.isCancelled {
                if let model = self { await step(model) } else { return }
                try? await Task.sleep(for: .seconds(seconds))
            }
        }
    }

    /// The helper to switch to is whichever `atlas` reports.
    public func findHelper() async {
        struct Reply: Decodable {
            struct Account: Decodable {
                let name: String
                let helper: Bool?
            }
            let accounts: [Account]
        }
        guard let commands else { return }
        helper = await commands.run(["switch", "--list"]).decode(Reply.self)?.accounts.first { $0.helper == true }?.name
    }

    /// Read the status. A reading that was cancelled, or overtaken by a
    /// newer one, shows nothing.
    func readStatus() async {
        guard let service else { return }
        statusReading += 1
        let number = statusReading
        let latest = try? await service.status()
        guard number == statusReading, !Task.isCancelled else { return }
        status = latest ?? ViewerStatus()
    }

    /// Read the desktop, under the same rule.
    func readFrame() async {
        guard let service else { return }
        frameReading += 1
        let number = frameReading
        let shown = try? await service.frame()
        guard number == frameReading, !Task.isCancelled else { return }
        // A failed capture never leaves an old desktop on show.
        image = shown.flatMap { NSImage(data: $0.png) }
        placeholder = "The helper’s screen is not available"
    }

    // MARK: Actions

    func say(_ text: String?, working: Bool = false) {
        message = text
        self.working = working
    }

    private func perform(_ action: Action, working: String, done: String) async {
        guard let commands, !self.working else { return }
        say(working, working: true)
        let outcome = await commands.run(action.arguments)
        say(outcome.ok ? done : outcome.message)
        await readStatus()
    }

    /// The emergency stop: never waits for anything else.
    public func releaseControl() async {
        guard let service else { return }
        say("Releasing control…", working: true)
        do {
            try await service.revoke()
            say("Control released.")
        } catch {
            say(error.localizedDescription)
        }
        await readStatus()
    }

    public func switchToHelper() async {
        guard let helper else { return }
        await perform(.switchToHelper(helper), working: "Switching…", done: "Switched")
    }

    public func recover() async {
        await perform(.recover, working: "Recovering the helper…", done: "The helper is ready again")
    }

    public func setLogin(_ on: Bool) {
        do {
            try loginItem?.set(on)
        } catch {
            say(error.localizedDescription)
        }
        atLogin = loginItem?.enabled == true
    }

    // MARK: Drawing

    /// A panel fixed at one state, for drawing it.
    public static func still(
        _ status: ViewerStatus, image: NSImage?, message: String? = nil, everything: Bool = false
    ) -> PanelModel {
        let model = PanelModel(service: nil, commands: nil, loginItem: nil)
        model.status = status
        model.image = image
        model.message = message
        if everything {
            model.helper = "helper"
            model.loginAvailable = true
        }
        return model
    }
}
