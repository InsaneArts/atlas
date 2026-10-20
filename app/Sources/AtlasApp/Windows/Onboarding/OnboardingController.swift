import AtlasKit
import AtlasUI
import Foundation
import Observation

/// The setup window's one observed object: the facts `atlas onboarding status` reports (read again every moment
/// while the window is open), the person's choices, and what is running now. Every action is an `atlas` command; the
/// window holds no policy of its own, and ``OnboardingPresenter`` decides what it shows.
///
/// What the window shows is written in this file alone, so the window's shell and view change it only through
/// ``send(_:)`` and ``update(_:)``.
@MainActor
@Observable
final class OnboardingController {
  /// How often the facts are read while the window is open: 1.5 s, soon enough that a switch turned on in System
  /// Settings, or a log-in in the helper, shows without the person waiting.
  static let readEvery: Duration = .seconds(1.5)

  /// Everything the window shows, as one value.
  private(set) var snapshot = OnboardingSnapshot()
  /// The window's work is done: setup finished, or the helper locked for the switch back. The window closes itself.
  private(set) var closeRequested = false

  /// What the controller reaches outside itself.
  @ObservationIgnored let dependencies: OnboardingDependencies
  /// The reading loop, while the window is open.
  @ObservationIgnored private var polling: Task<Void, Never>?
  /// Whether a reading runs now.
  @ObservationIgnored private var reading = false
  /// A reading asked for while one ran: done as soon as it ends.
  @ObservationIgnored private var readAgain = false
  /// Whether the helper's "set up here" notice was last told the window is open.
  @ObservationIgnored private var markedOpen = false
  /// The mark being set now, if any.
  @ObservationIgnored private var marking: Task<Void, Never>?
  /// The window closed: nothing new starts, and late replies are dropped.
  @ObservationIgnored private var closed = false

  /// Creates a controller that has read nothing yet.
  ///
  /// - Parameter dependencies: The `atlas` command line and the system pieces it reaches.
  init(dependencies: OnboardingDependencies) {
    self.dependencies = dependencies
  }

  /// The `atlas` command line every action runs.
  var commands: any CommandRunning { dependencies.commands }

  /// The window as it should look now.
  var state: OnboardingState { OnboardingPresenter.state(snapshot) }

  /// Applies one of the window's events.
  ///
  /// - Parameter event: What the person did.
  func send(_ event: OnboardingEvent) {
    switch event {
    case .begin: snapshot.choices.started = true
    case .back: back()
    case .pick(let account): snapshot.picked = account
    case .continueChoice: Task { await useHelper() }
    case .createAccount: Task { await createAccount() }
    case .lockAndSwitch: Task { await act(.switchTo(snapshot.facts.helper.name), Words.locking) }
    case .checkAgain: refresh()
    case .retryFinish: retryFinish()
    case .continueFinishing: refresh()
    case .setAgent(let id, let on): setAgent(id, on: on)
    case .chooseProject: chooseProject()
    case .removeProject(let id): snapshot.projects.removeAll { $0.path == id }
    case .connectAgents: Task { await connectAgents() }
    case .setOpenAtLogin(let on): snapshot.opensAtLogin = on
    case .complete: Task { await complete() }
    case .allow: Task { await allow() }
    case .openSettings(let id):
      if let permission = Permission.allCases.first(where: { $0.id == id }) { openSettings(permission) }
    case .closeGuide: snapshot.guiding = nil
    case .switchBack: Task { await switchBack() }
    }
  }

  /// The account form, as the person edits it in place.
  ///
  /// - Parameter form: The form as it now reads.
  func update(_ form: OnboardingAccountForm) {
    snapshot.form = form
  }

  // MARK: Reading

  /// Reads the facts now and then every ``readEvery`` until ``stop()``.
  func watch() {
    guard polling == nil else { return }
    closed = false
    polling = Task { [weak self] in
      // Ends once cancelled, or once the controller is gone.
      while !Task.isCancelled, let controller = self {
        await controller.read()
        try? await Task.sleep(for: Self.readEvery)
      }
    }
  }

  /// Stops reading.
  func stop() {
    polling?.cancel()
    polling = nil
  }

  /// Reads the facts again soon: now, or right after the reading under way.
  func refresh() {
    guard !closed else { return }
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
    let outcome = await commands.run(.onboardingStatus)
    reading = false
    guard !closed else { return }
    if outcome.ok, let facts = OnboardingFacts(json: outcome.reply) {
      snapshot.facts = facts
      if !snapshot.loaded {
        snapshot.opensAtLogin = facts.opensAtLogin(enabledNow: dependencies.loginItem.isEnabled())
      }
      snapshot.loaded = true
      if snapshot.picked == nil { snapshot.picked = facts.defaultPick.map(OnboardingAccountPick.init(name:)) }
      advance()
    } else if !snapshot.loaded {
      snapshot.problem = outcome.ok ? OnboardingWords.notUnderstood : outcome.message
    }
    if readAgain {
      readAgain = false
      await read()
    }
  }

  /// Starts what a step needs as soon as it is shown, and lets the guide step aside once it is done.
  private func advance() {
    guard !closed else { return }
    let step = snapshot.step
    switch step {
    case .finishing where snapshot.finish == .idle: runFinish()
    case .agents where snapshot.agents.isEmpty: loadAgents()
    case .allow, .permissions, .helperDone: markWindow(open: true)
    default: break
    }
    if let permission = snapshot.guiding, !permission.guideStays(on: step, asker: snapshot.facts.asker) {
      snapshot.guiding = nil
    }
  }

  /// Runs one command, with its words shown while it runs; one at a time, and none once the window has closed.
  ///
  /// A command already running when the window closes settles, but its outcome is dropped, so a workflow awaiting it
  /// stops there and starts no further command.
  ///
  /// - Parameters:
  ///   - action: The command.
  ///   - words: What to show while it runs.
  ///   - input: What to write to its standard input, if anything: a secret travels this way.
  /// - Returns: The command's outcome, or nil when another one runs or the window has closed.
  @discardableResult
  func act(_ action: Action, _ words: String, input: Data? = nil) async -> Outcome? {
    guard !closed, snapshot.working == nil else { return nil }
    snapshot.working = words
    snapshot.problem = nil
    let outcome = await commands.run(action, input: input)
    snapshot.working = nil
    guard !closed else { return nil }
    if !outcome.ok { snapshot.problem = outcome.message }
    refresh()
    return outcome
  }

  /// The window is closing: stop reading, start nothing new, and clear the helper's mark once a mark under way has
  /// settled, so the clearing lands last.
  func close() async {
    closed = true
    stop()
    await marking?.value
    if snapshot.facts.role == .helper {
      _ = await commands.run(.markWindow(open: false))
    }
  }

  // MARK: The person's side

  /// Back to the screen before: the welcome from the choice, the choice from the form, the agents from Done.
  private func back() {
    switch snapshot.step {
    case .chooseHelper: snapshot.choices.started = false
    case .createAccount: snapshot.choices.creating = false
    case .done: snapshot.choices.agentsDone = false
    default: break
    }
  }

  /// Continue on the choice: the form for a new account, else set the picked account up as the helper.
  func useHelper() async {
    switch snapshot.picked {
    case .newAccount: snapshot.choices.creating = true
    case .existing(let name): await act(.chooseHelper(name), OnboardingWords.settingUp(name))
    case nil: break
    }
  }

  /// Creates the account. The password reaches macOS on standard input, never in the arguments, and leaves the form
  /// as it is handed over.
  func createAccount() async {
    let form = snapshot.form
    guard form.canCreate, snapshot.working == nil else { return }
    var secret = Data(form.password.utf8)
    secret.append(0x0A)
    // Cleared from the window as soon as it is handed to macOS.
    snapshot.form.password = ""
    snapshot.form.verify = ""
    await act(
      .createAccount(account: form.accountName, fullName: form.fullName),
      OnboardingWords.creating,
      input: secret
    )
    secret.resetBytes(in: 0..<secret.count)
  }

  /// Runs the end-to-end check, then reads the facts again.
  private func runFinish() {
    snapshot.finish = .running
    Task {
      let outcome = await commands.run(.finishSetup)
      guard !closed else { return }
      snapshot.finish = outcome.ok ? .passed : .failed(outcome.message)
      refresh()
    }
  }

  /// Try Again after a failed check.
  private func retryFinish() {
    snapshot.finish = .idle
    runFinish()
  }

  /// Lists the agents Atlas can connect, then asks `atlas agents status` which are installed and connected.
  func loadAgents() {
    snapshot.agents = AgentChoice.all
    Task {
      let rows =
        await commands.run(.agentsStatus)
        .decode(AgentsStatus.self)?
        .agents ?? []
      guard !closed else { return }
      snapshot.agents = AgentChoice.merged(snapshot.agents, with: rows)
      snapshot.agentsLoaded = true
    }
  }

  /// Turns an agent's switch on or off.
  ///
  /// - Parameters:
  ///   - id: The agent's id, as the screen names it.
  ///   - on: Whether it is on.
  private func setAgent(_ id: String, on: Bool) {
    guard let index = snapshot.agents.firstIndex(where: { $0.id.rawValue == id }) else { return }
    snapshot.agents[index].on = on
  }

  /// Asks for project folders to teach, adding each new one. A folder is known by its path with links resolved, so
  /// one folder chosen twice, by another spelling or through a link, is added once.
  private func chooseProject() {
    for url in dependencies.chooseFolders() {
      let folder = url.standardizedFileURL.resolvingSymlinksInPath()
      if !snapshot.projects.contains(where: { $0.path == folder.path }) { snapshot.projects.append(folder) }
    }
  }

  /// Connects the chosen agents, then teaches each chosen project; a project that cannot be taught is named, and the
  /// rest still follow.
  func connectAgents() async {
    let chosen = AgentChoice.toConnect(snapshot.agents)
    if !chosen.isEmpty {
      guard let outcome = await act(.installAgents(chosen), OnboardingWords.connecting), outcome.ok else { return }
      for index in snapshot.agents.indices where chosen.contains(snapshot.agents[index].id) {
        snapshot.agents[index].connected = true
      }
    }
    var failed: [String] = []
    for project in snapshot.projects where !snapshot.taught.contains(project) {
      guard
        let outcome = await act(.teachProject(path: project.path), OnboardingWords.teaching(project.lastPathComponent))
      else { return }
      if outcome.ok {
        snapshot.taught.append(project)
      } else {
        failed.append(OnboardingWords.notTaught(project.lastPathComponent, outcome.message))
      }
    }
    snapshot.choices.agentsDone = true
    if !failed.isEmpty { snapshot.problem = failed.joined(separator: " ") }
  }

  /// Done: set Open at Login, open Atlas, then close the window; on any failure the window stays, saying why.
  func complete() async {
    guard let login = await act(.setOpensAtLogin(snapshot.opensAtLogin), OnboardingWords.finishing),
      login.ok,
      let started = await act(.openMenuBar, OnboardingWords.opening),
      started.ok
    else { return }
    closeRequested = true
  }

  // MARK: The helper's side

  /// Allows exactly the account shown: a request changed since is refused.
  func allow() async {
    let asker = snapshot.facts.asker
    await act(.allow(uid: asker.uid, name: asker.name), OnboardingWords.settingUpHere)
  }

  /// Opens the permission's Privacy & Security page, with the guide panel beside it.
  ///
  /// - Parameter permission: The permission to allow.
  private func openSettings(_ permission: Permission) {
    if let url = permission.settingsURL { dependencies.openURL(url) }
    snapshot.guiding = permission
  }

  /// Locks this account for the switch back, to the account that asked, or with none named when no one asked; the
  /// window's work here is then done.
  func switchBack() async {
    let name = snapshot.facts.asker.name
    let action: Action = name.isEmpty ? .lockScreen : .switchTo(name)
    if let outcome = await act(action, Words.locking), outcome.ok { closeRequested = true }
  }

  /// While open in the helper, Atlas's own "set up here" notice stays quiet; one mark at a time, tried again on the
  /// next reading.
  ///
  /// - Parameter open: Whether the window is open.
  private func markWindow(open: Bool) {
    guard snapshot.facts.role == .helper, !closed, marking == nil, open != markedOpen else { return }
    marking = Task {
      let marked = await commands.run(.markWindow(open: open)).ok
      marking = nil
      if marked { markedOpen = open }
    }
  }
}
