import AppKit
import AtlasKit
import Observation

/// One of the two permissions Atlas needs in the helper, and where System
/// Settings keeps its switch.
enum Permission: String, CaseIterable, Sendable {
    case accessibility = "Accessibility"
    case screenRecording = "Screen Recording"

    var title: String { rawValue }

    /// "1 of 2", as the guide panel counts them.
    var number: Int { (Permission.allCases.firstIndex(of: self) ?? 0) + 1 }

    /// The Privacy & Security page that holds the switch.
    var settingsURL: URL? {
        let anchor =
            switch self {
            case .accessibility: "Privacy_Accessibility"
            case .screenRecording: "Privacy_ScreenCapture"
            }
        return URL(string: "x-apple.systempreferences:com.apple.preference.security?\(anchor)")
    }

    /// Whether `asker` (this account, in the helper) has it.
    func granted(to asker: OnboardingFacts.Asker) -> Bool {
        switch self {
        case .accessibility: asker.accessibility
        case .screenRecording: asker.screenCapture
        }
    }
}

/// The onboarding window's state: the facts `atlas onboarding status`
/// reports (read again every moment while the window is open), the person's
/// choices, and what is running now. Every action is an `atlas` command; the
/// window holds no policy of its own.
@MainActor
@Observable
public final class OnboardingModel {
    /// How often the facts are read while the window is open.
    static let readEvery: Duration = .seconds(1.5)

    private(set) var facts = OnboardingFacts()
    var choices = OnboardingChoices()
    private(set) var loaded = false
    /// The command running now, if any, in words.
    private(set) var working: String?
    /// What went wrong last, in one line.
    var problem: String?

    // Choosing a helper.
    var picked: String?
    var fullName = "Atlas"
    var accountName = "atlas"
    var password = ""
    var verify = ""

    // Finishing.
    enum Finish: Equatable, Sendable { case idle, running, passed, failed(String) }
    private(set) var finish = Finish.idle

    // Agents.
    struct AgentChoice: Identifiable, Equatable, Sendable {
        let id: String
        let title: String
        var found = false
        var connected = false
        var on = false
    }
    var agents: [AgentChoice] = []
    /// Whether `atlas agents status` has said which agents are installed.
    private(set) var agentsLoaded = false
    var projects: [URL] = []
    private(set) var taught: [URL] = []

    // Done.
    var openAtLogin = true

    /// The permission the guide panel beside System Settings is about, while it is open.
    var guiding: Permission?

    /// Closes the window (set by it).
    @ObservationIgnored var close: (@MainActor () -> Void)?

    @ObservationIgnored let commands: any CommandRunning
    @ObservationIgnored private var polling: Task<Void, Never>?
    @ObservationIgnored private var reading = false
    /// A reading asked for while one ran: done as soon as it ends.
    @ObservationIgnored private var readAgain = false
    @ObservationIgnored private var markedOpen = false
    @ObservationIgnored private var marking = false
    /// The window closed: nothing new starts, and late replies are dropped.
    @ObservationIgnored private var closed = false
    /// Drawing only (render tests): never reads or runs anything.
    @ObservationIgnored let still: Bool

    public init(commands: any CommandRunning, still: Bool = false) {
        self.commands = commands
        self.still = still
    }

    var step: OnboardingStep { onboardingStep(facts, choices) }

    // MARK: Reading

    /// Read the facts now and then every `readEvery` until `stop()`.
    func start() {
        guard !still, polling == nil else { return }
        closed = false
        polling = Task { [weak self] in
            // Ends once cancelled, or once the model is gone.
            while !Task.isCancelled, let model = self {
                await model.read()
                try? await Task.sleep(for: Self.readEvery)
            }
        }
    }

    func stop() {
        polling?.cancel()
        polling = nil
    }

    /// Read the facts again soon: now, or right after the reading under way.
    func refresh() {
        guard !still, !closed else { return }
        if reading {
            readAgain = true
        } else {
            Task { await read() }
        }
    }

    /// One reading at a time, so an old one never lands after a newer one.
    private func read() async {
        guard !reading else {
            readAgain = true
            return
        }
        reading = true
        let outcome = await commands.run(["onboarding", "status"])
        reading = false
        guard !closed else { return }
        if outcome.ok, let facts = OnboardingFacts(json: outcome.reply) {
            self.facts = facts
            if !loaded {
                // The person's own choice stands; the first setup turns it on.
                openAtLogin = facts.loginDecided ? LoginItem().enabled : true
            }
            loaded = true
            if picked == nil {
                picked = facts.helper.exists ? facts.helper.name : (facts.candidates.isEmpty ? "" : nil)
            }
            advance()
        } else if !loaded {
            problem = outcome.ok ? "Atlas gave an answer this window does not understand" : outcome.message
        }
        if readAgain {
            readAgain = false
            await read()
        }
    }

    /// Start what a step needs as soon as it is shown.
    private func advance() {
        guard !closed else { return }
        switch step {
        case .finishing where finish == .idle: runFinish()
        case .agents where agents.isEmpty: loadAgents()
        case .allow, .permissions, .helperDone: markWindow(open: true)
        default: break
        }
        // The switch is on (or the step moved on): the side panel steps aside.
        if let permission = guiding, step != .permissions || granted(permission) {
            guiding = nil
        }
    }

    func granted(_ permission: Permission) -> Bool {
        permission.granted(to: facts.asker)
    }

    /// Run one command, with its words shown while it runs; one at a time.
    @discardableResult
    func act(_ arguments: [String], _ words: String, input: Data? = nil) async -> Outcome? {
        guard working == nil, !still else { return nil }
        working = words
        problem = nil
        let outcome = await commands.run(arguments, input: input)
        working = nil
        if !outcome.ok { problem = outcome.message }
        refresh()
        return outcome
    }

    // MARK: The person's account

    func begin() { choices.started = true }

    func useHelper() async {
        guard let picked else { return }
        if picked.isEmpty {
            choices.creating = true
        } else {
            await act(["onboarding", "choose", picked], "Setting up \(picked)…")
        }
    }

    var passwordProblem: String? {
        if password.utf8.count > 256 { return "Use a password of at most 256 characters." }
        if password.unicodeScalars.contains(where: { $0.properties.generalCategory == .control }) {
            return "Use a password without control characters."
        }
        if password.isEmpty || verify.isEmpty { return nil }
        return password == verify ? nil : "The two passwords are different."
    }

    var canCreate: Bool {
        !fullName.isEmpty && !accountName.isEmpty && !password.isEmpty && password == verify
            && passwordProblem == nil
    }

    func createAccount() async {
        guard canCreate, working == nil else { return }
        var secret = Data(password.utf8)
        secret.append(0x0A)
        // Cleared from the window as soon as it is handed to macOS.
        password = ""
        verify = ""
        await act(
            ["onboarding", "create", "--account", accountName, "--full-name", fullName],
            "Creating the account… Confirm in the macOS dialog.", input: secret)
        secret.resetBytes(in: 0..<secret.count)
    }

    func lockAndSwitch() async {
        await act(["switch", facts.helper.name, "--yes"], "Locking the screen…")
    }

    private func runFinish() {
        finish = .running
        Task {
            let outcome = await commands.run(["onboarding", "finish"])
            finish = outcome.ok ? .passed : .failed(outcome.message)
            refresh()
        }
    }

    func retryFinish() {
        finish = .idle
        runFinish()
    }

    private func loadAgents() {
        agents = [
            AgentChoice(id: "claude", title: "Claude Code"),
            AgentChoice(id: "codex", title: "Codex"),
        ]
        Task {
            struct Reply: Decodable {
                struct Row: Decodable {
                    let agent: String
                    let installed: Bool?
                    let connected: Bool?
                }
                let agents: [Row]
            }
            let rows = await commands.run(["agents", "status"]).decode(Reply.self)?.agents ?? []
            for row in rows {
                guard let index = agents.firstIndex(where: { $0.id == row.agent }) else { continue }
                agents[index].found = row.installed == true
                agents[index].connected = row.connected == true
                agents[index].on = agents[index].found
            }
            agentsLoaded = true
        }
    }

    func chooseProject() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = true
        panel.prompt = "Teach"
        panel.message = "Choose a project folder. Atlas adds a short section to its AGENTS.md."
        guard panel.runModal() == .OK else { return }
        for url in panel.urls where !projects.contains(url) { projects.append(url) }
    }

    func removeProject(_ url: URL) {
        projects.removeAll { $0 == url }
    }

    /// Connect the chosen agents, then teach each chosen project; a project
    /// that cannot be taught is named, and the rest still follow.
    func connectAgents() async {
        let chosen = agents.filter { $0.on && $0.found && !$0.connected }
        if !chosen.isEmpty {
            guard
                let outcome = await act(
                    ["agents", "install"] + chosen.map { "--\($0.id)" }, "Connecting your agents…"),
                outcome.ok
            else { return }
            for agent in chosen {
                if let index = agents.firstIndex(where: { $0.id == agent.id }) { agents[index].connected = true }
            }
        }
        var failed: [String] = []
        for project in projects where !taught.contains(project) {
            guard
                let outcome = await act(["agents", "project", project.path], "Teaching \(project.lastPathComponent)…")
            else { return }
            if outcome.ok {
                taught.append(project)
            } else {
                failed.append("\(project.lastPathComponent): \(outcome.message).")
            }
        }
        choices.agentsDone = true
        if !failed.isEmpty { problem = failed.joined(separator: " ") }
    }

    /// Done: set Open at Login, open Atlas, then close the window; on any
    /// failure the window stays, saying why.
    func complete() async {
        guard let login = await act(["onboarding", "login", openAtLogin ? "on" : "off"], "Finishing…"), login.ok,
            let started = await act(["menubar"], "Opening Atlas…"), started.ok
        else { return }
        close?()
    }

    // MARK: The helper's account

    /// Allow exactly the account shown: a request changed since is refused.
    func allow() async {
        let asker = facts.asker
        await act(
            ["onboarding", "accept", "--controller-uid", String(asker.uid), "--controller-name", asker.name],
            "Setting Atlas up in this account…")
    }

    /// Open the permission's Privacy & Security page, with the guide panel beside it.
    func openSettings(_ permission: Permission) {
        if let url = permission.settingsURL { NSWorkspace.shared.open(url) }
        guiding = permission
    }

    /// The first permission still to allow, if any.
    var nextPermission: Permission? {
        Permission.allCases.first { !granted($0) }
    }

    /// Lock this account for the switch back; the window's work here is done.
    func switchBack() async {
        let name = facts.asker.name
        let arguments = name.isEmpty ? ["switch", "--lock", "--yes"] : ["switch", name, "--yes"]
        if let outcome = await act(arguments, "Locking the screen…"), outcome.ok { close?() }
    }

    /// While open in the helper, Atlas's own "set up here" notice stays
    /// quiet; one mark at a time, tried again on the next reading.
    private func markWindow(open: Bool) {
        guard facts.role == .helper, !still, !closed, !marking, open != markedOpen else { return }
        marking = true
        Task {
            let marked = await commands.run(["onboarding", "window", open ? "open" : "closed"]).ok
            marking = false
            if marked { markedOpen = open }
        }
    }

    /// The window closed: stop reading, start nothing new, and clear the mark.
    func closing() async {
        closed = true
        stop()
        if facts.role == .helper, !still {
            _ = await commands.run(["onboarding", "window", "closed"])
        }
    }

    // MARK: Drawing

    /// A model fixed at one state, for drawing the screens.
    static func still(
        _ facts: OnboardingFacts, _ choices: OnboardingChoices = .init(),
        finish: Finish = .idle, working: String? = nil, guiding: Permission? = nil
    ) -> OnboardingModel {
        let model = OnboardingModel(commands: Commands(executable: URL(fileURLWithPath: "/usr/bin/false")), still: true)
        model.facts = facts
        model.choices = choices
        model.loaded = true
        model.finish = finish
        model.working = working
        model.guiding = guiding
        return model
    }
}
