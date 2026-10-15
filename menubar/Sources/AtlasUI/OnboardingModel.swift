import AppKit
import AtlasKit
import SwiftUI

/// The onboarding window's state: the facts `atlas onboarding status`
/// reports (read again every moment while the window is open), the
/// person's choices, and what is running now. Every action is an `atlas`
/// command; the window holds no policy of its own.
@MainActor
public final class OnboardingModel: ObservableObject {
    @Published public private(set) var facts = OnboardingFacts()
    @Published public var choices = OnboardingChoices()
    @Published public private(set) var loaded = false
    /// The command running now, if any, in words.
    @Published public private(set) var working: String?
    /// What went wrong last, in one line.
    @Published public var problem: String?

    // Choosing a helper.
    @Published public var picked: String?
    @Published public var fullName = "Atlas Helper"
    @Published public var accountName = "atlas"
    @Published public var password = ""
    @Published public var verify = ""

    // Finishing.
    public enum Finish: Equatable { case idle, running, passed, failed(String) }
    @Published public private(set) var finish = Finish.idle

    // Agents.
    public struct AgentChoice: Identifiable, Equatable {
        public let id: String
        public let title: String
        public var found: Bool
        public var connected: Bool
        public var on: Bool
    }
    @Published public var agents: [AgentChoice] = []
    @Published public var projects: [URL] = []
    @Published public var taught: [URL] = []

    // Done.
    @Published public var openAtLogin = true

    /// The side panel beside System Settings, while it is open.
    @Published public var guiding: String?

    let commands: AtlasKit.Commands
    private var timer: Timer?
    private var reading = false
    /// A reading asked for while one ran: done as soon as it ends.
    private var readAgain = false
    private var markedOpen = false
    /// Closes the window (set by it).
    public var close: (@MainActor () -> Void)?
    /// Render-only: never read or run anything.
    let still: Bool

    public init(commands: AtlasKit.Commands, still: Bool = false) {
        self.commands = commands
        self.still = still
    }

    public var step: OnboardingStep { onboardingStep(facts, choices) }

    // MARK: Reading

    public func start() {
        guard !still else { return }
        refresh()
        timer = Timer.scheduledTimer(withTimeInterval: 1.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    public func stop() {
        timer?.invalidate()
        timer = nil
    }

    /// Read the facts again; one reading at a time, so an old one never
    /// lands after a newer one.
    public func refresh() {
        guard !still else { return }
        guard !reading else {
            readAgain = true
            return
        }
        reading = true
        commands.run(["onboarding", "status"]) { [weak self] outcome in
            guard let self else { return }
            reading = false
            defer {
                if readAgain {
                    readAgain = false
                    refresh()
                }
            }
            if outcome.ok {
                facts = OnboardingFacts(outcome.reply)
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
                problem = outcome.message
            }
        }
    }

    /// Start what a step needs as soon as it is shown.
    func advance() {
        switch step {
        case .finishing where finish == .idle: runFinish()
        case .agents where agents.isEmpty: loadAgents()
        case .permissions, .helperDone, .allow: markWindow(open: true)
        default: break
        }
        if let pane = guiding, step != .permissions || granted(pane) {
            // The switch is on: the side panel steps aside.
            _ = pane
            guiding = nil
        }
    }

    func granted(_ pane: String) -> Bool {
        pane == "Accessibility" ? facts.asker.accessibility : facts.asker.screenCapture
    }

    /// Run one command, with its words shown while it runs.
    func act(
        _ arguments: [String], _ words: String, input: Data? = nil,
        then: @escaping @MainActor (Outcome) -> Void = { _ in }
    ) {
        guard working == nil, !still else { return }
        working = words
        problem = nil
        commands.run(arguments, input: input) { [weak self] outcome in
            guard let self else { return }
            working = nil
            if !outcome.ok { problem = outcome.message }
            then(outcome)
            refresh()
        }
    }

    // MARK: The person's account

    public func begin() { choices.started = true }

    public func useHelper() {
        guard let picked else { return }
        if picked.isEmpty {
            choices.creating = true
        } else {
            act(["onboarding", "choose", picked], "Setting up \(picked)…")
        }
    }

    public var passwordProblem: String? {
        if password.utf8.count > 256 { return "Use a password of at most 256 characters." }
        if password.isEmpty || verify.isEmpty { return nil }
        return password == verify ? nil : "The two passwords are different."
    }

    public var canCreate: Bool {
        !fullName.isEmpty && !accountName.isEmpty && !password.isEmpty && password == verify
            && password.utf8.count <= 256 && !password.contains("\n")
    }

    public func createAccount() {
        guard canCreate else { return }
        var secret = Data(password.utf8)
        secret.append(0x0A)
        act(
            ["onboarding", "create", "--account", accountName, "--full-name", fullName],
            "Creating \(fullName)… Confirm in the macOS dialog.", input: secret
        ) { [weak self] outcome in
            guard let self else { return }
            // Never kept a moment longer than needed.
            password = ""
            verify = ""
        }
    }

    public func lockAndSwitch() {
        act(["switch", facts.helper.name, "--yes"], "Locking the screen…")
    }

    func runFinish() {
        finish = .running
        commands.run(["onboarding", "finish"]) { [weak self] outcome in
            guard let self else { return }
            finish = outcome.ok ? .passed : .failed(outcome.message)
            refresh()
        }
    }

    public func retryFinish() {
        finish = .idle
        runFinish()
    }

    func loadAgents() {
        agents = [
            AgentChoice(id: "claude", title: "Claude Code", found: false, connected: false, on: false),
            AgentChoice(id: "codex", title: "Codex", found: false, connected: false, on: false),
        ]
        commands.run(["agents", "status"]) { [weak self] outcome in
            guard let self else { return }
            let rows = outcome.reply["agents"] as? [[String: Any]] ?? []
            for row in rows {
                guard let id = row["agent"] as? String,
                    let index = agents.firstIndex(where: { $0.id == id })
                else { continue }
                agents[index].found = row["installed"] as? Bool ?? false
                agents[index].connected = row["connected"] as? Bool ?? false
                agents[index].on = agents[index].found
            }
        }
    }

    public func chooseProject() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = true
        panel.prompt = "Teach"
        panel.message = "Choose a project folder. Atlas adds a short section to its AGENTS.md."
        if panel.runModal() == .OK {
            for url in panel.urls where !projects.contains(url) { projects.append(url) }
        }
    }

    public func connectAgents() {
        let chosen = agents.filter { $0.on && $0.found && !$0.connected }
        let teach = projects.filter { !taught.contains($0) }
        let next: @MainActor () -> Void = { [weak self] in
            guard let self else { return }
            teachProjects(teach, failed: []) { [weak self] in self?.choices.agentsDone = true }
        }
        if chosen.isEmpty {
            next()
        } else {
            act(["agents", "install"] + chosen.map { "--\($0.id)" }, "Connecting your agents…") {
                [weak self] outcome in
                guard let self else { return }
                if outcome.ok {
                    for agent in chosen {
                        if let index = agents.firstIndex(where: { $0.id == agent.id }) {
                            agents[index].connected = true
                        }
                    }
                    next()
                }
            }
        }
    }

    /// Teach each project in turn; one that cannot be taught is named, and
    /// the rest (and the next screen) still follow.
    func teachProjects(_ remaining: [URL], failed: [String], then: @escaping @MainActor () -> Void) {
        guard let first = remaining.first else {
            then()
            if !failed.isEmpty { problem = failed.joined(separator: " ") }
            return
        }
        act(["agents", "project", first.path], "Teaching \(first.lastPathComponent)…") { [weak self] outcome in
            guard let self else { return }
            var failed = failed
            if outcome.ok {
                taught.append(first)
            } else {
                failed.append("\(first.lastPathComponent): \(outcome.message).")
            }
            teachProjects(Array(remaining.dropFirst()), failed: failed, then: then)
        }
    }

    public func removeProject(_ url: URL) {
        projects.removeAll { $0 == url }
    }

    /// Done: set Open at Login, open the menu bar, then close this window;
    /// on any failure the window stays, saying why.
    public func complete(_ close: @escaping @MainActor () -> Void) {
        act(["onboarding", "login", openAtLogin ? "on" : "off"], "Finishing…") { [weak self] login in
            guard let self, login.ok else { return }
            working = "Opening the menu bar…"
            commands.run(["menubar"]) { [weak self] started in
                guard let self else { return }
                working = nil
                if started.ok {
                    close()
                } else {
                    problem = started.message
                }
            }
        }
    }

    // MARK: The helper's account

    /// Allow exactly the account shown: a request changed since is refused.
    public func allow() {
        let asker = facts.asker
        act(
            [
                "onboarding", "accept", "--controller-uid", String(asker.uid),
                "--controller-name", asker.name,
            ], "Setting Atlas up in this account…")
    }

    /// Open a Privacy & Security page, with the side panel beside it.
    public func openSettings(_ pane: String) {
        let anchor = pane == "Accessibility" ? "Privacy_Accessibility" : "Privacy_ScreenCapture"
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(anchor)") {
            NSWorkspace.shared.open(url)
        }
        guiding = pane
    }

    public var nextPermission: String? {
        if !facts.asker.accessibility { return "Accessibility" }
        if !facts.asker.screenCapture { return "Screen Recording" }
        return nil
    }

    /// Lock this account for the switch back; the window's work here is done.
    public func switchBack() {
        let arguments = facts.asker.name.isEmpty ? ["switch", "--lock", "--yes"] : ["switch", facts.asker.name, "--yes"]
        act(arguments, "Locking the screen…") { [weak self] outcome in
            if outcome.ok { self?.close?() }
        }
    }

    /// While open in the helper, Atlas's own "set up here" notice stays quiet.
    func markWindow(open: Bool) {
        guard facts.role == .helper, !still, open != markedOpen else { return }
        commands.run(["onboarding", "window", open ? "open" : "closed"]) { [weak self] outcome in
            // Tried again on the next reading if it did not take.
            if outcome.ok { self?.markedOpen = open }
        }
    }

    public func closing() {
        stop()
        if facts.role == .helper { markWindow(open: false) }
    }

    // MARK: Rendering

    /// A model fixed at one state, for rendering the screens.
    public static func preview(_ facts: OnboardingFacts, _ choices: OnboardingChoices = .init()) -> OnboardingModel {
        let model = OnboardingModel(commands: AtlasKit.Commands(executable: URL(fileURLWithPath: "/usr/bin/false")), still: true)
        model.facts = facts
        model.choices = choices
        model.loaded = true
        return model
    }

    public func setForPreview(finish: Finish? = nil, working: String? = nil, guiding: String? = nil) {
        if let finish { self.finish = finish }
        self.working = working
        self.guiding = guiding
    }
}
