import AppKit
import AtlasKit
import Observation

/// What the menu bar panel shows: the latest status, the helper's desktop
/// and the last action's outcome. Every action is an `atlas` command or a
/// request to the viewer service; the panel holds no policy of its own.
@MainActor
@Observable
final class PanelModel {
    /// Seconds between status readings with the panel open and closed; the
    /// desktop is read at the open rate while the panel is open.
    static let openEvery = 2.0
    static let closedEvery = 5.0

    private(set) var status = ViewerStatus()
    private(set) var image: NSImage?
    private(set) var placeholder = "Waiting for the helper's screen…"
    /// What the last action said, and whether it still runs.
    private(set) var message: String?
    private(set) var working = false
    /// The helper's short name, once `atlas switch --list` reports it.
    private(set) var helper: String?
    private(set) var atLogin = false
    private(set) var loginAvailable = false
    /// Log Out Helper was chosen and waits for the person to confirm.
    private(set) var confirmingLogOut = false
    /// The last log-out was held up by an app in the helper.
    private(set) var forceOffered = false

    /// Opens the setup window (set by the app).
    @ObservationIgnored var gettingStarted: (@MainActor () -> Void)?
    /// Opens the viewer window (set by the app).
    @ObservationIgnored var openViewer: (@MainActor () -> Void)?

    @ObservationIgnored let service: (any ViewerService)?
    @ObservationIgnored let commands: (any CommandRunning)?
    @ObservationIgnored let loginItem: LoginItem?
    @ObservationIgnored private var polling: [Task<Void, Never>] = []
    /// Number each reading, so only the newest one started is shown.
    @ObservationIgnored private var statusReading = 0
    @ObservationIgnored private var frameReading = 0

    init(service: (any ViewerService)?, commands: (any CommandRunning)?, loginItem: LoginItem?) {
        self.service = service
        self.commands = commands
        self.loginItem = loginItem
        atLogin = loginItem?.enabled == true
        loginAvailable = loginItem?.available == true
    }

    var mode: Mode { Mode.of(status.overview.mode) }
    var overview: Overview { Overview(status) }
    var controlling: Bool { status.overview.mode == .controlling }
    var recovering: Bool { status.overview.mode == .recovering }
    var loggedOut: Bool { status.overview.mode == .loggedOut }

    /// Logging out needs a helper that answers with nothing left to settle.
    var logOutAllowed: Bool { [.observing, .controlling, .blocked].contains(status.overview.mode) }
    /// Logging out anyway also settles a log-out that an app held up.
    var forceAllowed: Bool { logOutAllowed || recovering }

    // MARK: Reading

    /// Read the status now and then at the open or closed rate, and the
    /// desktop at the open rate while open, until called again or `stop()`.
    /// The two run apart, so a slow capture never holds the status back.
    func watch(open: Bool) {
        stop()
        polling.append(repeating(every: open ? Self.openEvery : Self.closedEvery) { await $0.readStatus() })
        if open { polling.append(repeating(every: Self.openEvery) { await $0.readFrame() }) }
    }

    private func stop() {
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
    func findHelper() async {
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
        // A log-out asked for, or offered again, only while it still may run.
        if !working {
            confirmingLogOut = confirmingLogOut && logOutAllowed
            forceOffered = forceOffered && forceAllowed
        }
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
        placeholder = "The helper's screen is not available"
    }

    // MARK: Actions

    func say(_ text: String?, working: Bool = false) {
        message = text
        self.working = working
    }

    @discardableResult
    private func perform(_ action: Action, working: String, done: String) async -> Outcome? {
        guard let commands, !self.working else { return nil }
        say(working, working: true)
        let outcome = await commands.run(action.arguments)
        say(outcome.ok ? done : outcome.message)
        await readStatus()
        return outcome
    }

    /// The emergency stop: never waits for anything else.
    func releaseControl() async {
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

    func switchToHelper() async {
        guard let helper else { return }
        await perform(.switchTo(helper), working: "Switching…", done: "Switched")
    }

    /// Lock the screen so the person can log in to the helper there.
    func logIn() async {
        if helper == nil { await findHelper() }
        guard let helper else {
            say("Atlas couldn't find the helper account. Open Getting Started to check it.")
            return
        }
        await perform(
            .switchTo(helper), working: "Locking the screen…",
            done: "Log in to \(status.overview.title ?? helper), then switch back.")
    }

    func askToLogOut() {
        say(nil)
        forceOffered = false
        confirmingLogOut = true
    }

    func cancelLogOut() {
        confirmingLogOut = false
    }

    /// Log the helper out; with `force`, even past an app that holds it up.
    func logOut(force: Bool = false) async {
        guard force ? forceAllowed : logOutAllowed else { return }
        confirmingLogOut = false
        forceOffered = false
        // Control ends first: nothing of the person's stays held in the helper.
        if controlling, let service {
            do {
                try await service.revoke()
            } catch {
                say(error.localizedDescription)
                return
            }
        }
        let outcome = await perform(
            .logOut(force: force), working: "Logging out the helper…", done: "The helper is logged out.")
        forceOffered = outcome?.code == "logout-held"
    }

    func recover() async {
        await perform(.recover, working: "Recovering the helper…", done: "The helper is ready again.")
    }

    func setLogin(_ on: Bool) {
        do {
            try loginItem?.set(on)
        } catch {
            say(error.localizedDescription)
        }
        atLogin = loginItem?.enabled == true
    }

    // MARK: Drawing

    /// A panel fixed at one state, for drawing it; `everything` shows every
    /// action as on a set-up Mac.
    static func still(
        _ status: ViewerStatus, image: NSImage?, message: String? = nil, everything: Bool = false,
        confirming: Bool = false, forceOffered: Bool = false
    ) -> PanelModel {
        let model = PanelModel(service: nil, commands: nil, loginItem: nil)
        model.status = status
        model.image = image
        model.message = message
        model.confirmingLogOut = confirming
        model.forceOffered = forceOffered
        if everything {
            model.helper = "helper"
            model.loginAvailable = true
        }
        return model
    }
}
