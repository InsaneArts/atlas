import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasOnboarding
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// A fresh Mac's reply: no helper chosen, and no standard account that could be one. Shared with
/// `OnboardingControllerChoiceTests`.
let freshMac = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": false, "candidates": []}
  """

/// A set-up Mac's reply: the helper `atlas` is chosen, allowed and checked, so the agents come next.
/// Shared with `OnboardingControllerChoiceTests`.
let finishedMac = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": true, "finished": true, "candidates": [],
   "helper": {"exists": true, "name": "atlas", "administrator": false, "paired": true, "signedIn": true,
              "answering": true, "hidden": true, "permissions": {"screenCapture": true, "accessibility": true}}}
  """

/// A Mac whose helper is chosen, allowed and answering, but not checked yet: the end-to-end check runs next.
private let checkingMac = finishedMac.replacingOccurrences(of: #""finished": true"#, with: #""finished": false"#)

@Suite("OnboardingController")
struct OnboardingControllerTests {
  /// A controller that runs `commands` and reaches nothing else of the system.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line.
  ///   - opened: Records the URLs opened.
  ///   - loginItem: The scripted login item.
  ///   - installs: The agent installs it shares with another window; its own over `commands` when nil.
  ///   - clock: The clock its waits run on; by default one nobody advances.
  /// - Returns: The controller.
  static func controller(
    _ commands: ScriptedCommands,
    opened: OpenedURLs = OpenedURLs(),
    loginItem: ScriptedLoginItem = ScriptedLoginItem(),
    installs: AgentInstalls? = nil,
    clock: ManualClock = ManualClock()
  ) -> OnboardingController {
    OnboardingController(
      dependencies: OnboardingDependencies(
        commands: commands,
        openURL: { opened.urls.append($0) },
        loginItem: loginItem.seam,
        installs: installs ?? AgentInstalls(commands: commands),
        clock: clock
      )
    )
  }

  /// A controller that has read `reply` once.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line, answering `onboarding status`.
  ///   - installs: The agent installs it shares with another window; its own over `commands` when nil.
  /// - Returns: The controller, loaded.
  /// - Throws: The failed requirement, when the facts are never read.
  private static func loaded(
    _ commands: ScriptedCommands,
    installs: AgentInstalls? = nil
  ) async throws -> OnboardingController {
    let controller = controller(commands, installs: installs)
    controller.refresh()
    try await settle("the facts were never read") { controller.snapshot.loaded }
    return controller
  }

  /// A helper-side reply for the account `me`, with Atlas running and its two permissions as given.
  ///
  /// - Parameters:
  ///   - asker: The account that asked, or nil when no one did.
  ///   - accessibility: Whether Accessibility is on.
  ///   - screenCapture: Whether Screen Recording is on.
  /// - Returns: The reply.
  private static func helperReply(asker: String? = "me", accessibility: Bool, screenCapture: Bool = false) -> String {
    let controller = asker.map { #"{"uid": 501, "name": "\#($0)"}"# } ?? "null"
    return """
      {"ok": true, "role": "helper", "me": {"name": "atlas"}, "app": {"installed": true, "signed": true},
       "helper": {"controller": \(controller), "installed": true, "running": true,
                  "permissions": {"accessibility": \(accessibility), "screenCapture": \(screenCapture)}}}
      """
  }

  /// A helper-side reply while Atlas's job restarts: installed, not running, its permissions unknown.
  private static let restartingReply = """
    {"ok": true, "role": "helper", "me": {"name": "atlas"}, "app": {"installed": true, "signed": true},
     "helper": {"controller": {"uid": 501, "name": "me"}, "installed": true, "running": false, "permissions": null}}
    """

  @Test("a fresh Mac with no candidates picks a new account, and Continue opens the form")
  func aFreshMacPicksANewAccount() async throws {
    let controller = try await Self.loaded(ScriptedCommands(["onboarding status": freshMac]))
    #expect(controller.snapshot.step == .welcome)
    #expect(controller.snapshot.picked == .newAccount, "no candidates: a new account is picked")
    controller.send(.begin)
    await controller.useHelper()
    #expect(controller.snapshot.step == .createAccount)
  }

  @Test("Back leaves the form for the account choice, and the choice for the welcome")
  func backStepsBack() async throws {
    let controller = try await Self.loaded(ScriptedCommands(["onboarding status": freshMac]))
    controller.send(.begin)
    await controller.useHelper()
    controller.send(.back)
    #expect(controller.snapshot.step == .chooseHelper)
    controller.send(.back)
    #expect(controller.snapshot.step == .welcome)
  }

  @Test("a helper macOS could not ready says to open Atlas there, and one it readied says Atlas opens by itself")
  func anUnreadiedHelperSaysToOpenAtlas() async throws {
    let unreadied = ScriptedCommands([
      "onboarding status": freshMac, "onboarding create": #"{"ok": true, "readied": false}"#,
    ])
    let controller = Self.controller(unreadied)
    var form = controller.snapshot.form
    form.password = "hunter2"
    form.verify = "hunter2"
    controller.update(form)
    await controller.createAccount()
    #expect(!controller.snapshot.helperReadied)
    #expect(!OnboardingPresenter.switchToHelper(controller.snapshot).opensByItself)

    let readied = ScriptedCommands([
      "onboarding status": freshMac, "onboarding create": #"{"ok": true, "readied": true}"#,
    ])
    let other = Self.controller(readied)
    other.update(form)
    await other.createAccount()
    #expect(OnboardingPresenter.switchToHelper(other.snapshot).opensByItself)
  }

  @Test("the password reaches macOS on standard input and the form forgets it")
  func thePasswordTravelsOnStandardInput() async throws {
    let commands = ScriptedCommands(["onboarding status": freshMac, "onboarding create": #"{"ok": true}"#])
    let controller = Self.controller(commands)
    var form = controller.snapshot.form
    form.password = "hunter2"
    form.verify = "hunter2"
    controller.update(form)
    await controller.createAccount()
    let create = try #require(commands.ran.firstIndex { $0.starts(with: ["onboarding", "create"]) })
    #expect(commands.ran[create] == Action.createAccount(account: form.accountName, fullName: form.fullName).arguments)
    #expect(commands.inputs[create] == Data("hunter2\n".utf8))
    #expect(controller.snapshot.form.password.isEmpty && controller.snapshot.form.verify.isEmpty)
  }

  @Test("a failed command shows its message in one line, and nothing runs any more")
  func aFailedCommandIsShown() async throws {
    let withQA = freshMac.replacingOccurrences(of: #""candidates": []"#, with: #""candidates": [{"name": "qa"}]"#)
    let controller = try await Self.loaded(ScriptedCommands(["onboarding status": withQA]))
    controller.send(.pick(.existing("qa")))
    await controller.useHelper()
    #expect(controller.snapshot.problem == "unknown command.", "a command's message reads as a sentence")
    #expect(controller.snapshot.working == nil)
  }

  @Test("a closed window starts no reading")
  func aClosedWindowStartsNothing() async throws {
    let commands = ScriptedCommands(["onboarding status": freshMac])
    let controller = Self.controller(commands)
    await controller.close()
    controller.refresh()
    // The sentinel goes the same way: an open window over the same command line reads once, after any reading the
    // closed one started has set off for the command line.
    await afterQueuedWork()
    let open = Self.controller(commands)
    open.refresh()
    try await settle("the open window never read") { open.snapshot.loaded }
    #expect(commands.ran == [Action.onboardingStatus.arguments], "only the open window read")
    #expect(!controller.snapshot.loaded)
  }

  @Test("closing setup while a command runs lets it settle and moves on to nothing")
  func closingWhileACommandRunsMovesOnToNothing() async throws {
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
      "agents install": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands)
    try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
    // The window closes while the agents are installed.
    commands.whileRunning { arguments in
      if arguments.starts(with: ["agents", "install"]) { await controller.close() }
    }
    await controller.connectAgents()
    #expect(commands.ran.contains(Action.installAgents([.claude]).arguments), "the running command settled")
    #expect(!controller.snapshot.choices.agentsDone, "the closed window moved on to nothing")
  }

  @Test("closing the helper's window clears its mark only once a mark under way has settled")
  func closingClearsTheMarkLast() async throws {
    let open = Action.markWindow(open: true).arguments
    let closed = Action.markWindow(open: false).arguments
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = Self.controller(commands)
    let overlap = Overlap()
    // The person closes the window while the open mark runs; a close that did not wait for the mark would have
    // cleared it before the sentinel queued behind the close.
    commands.whileRunning { arguments in
      guard arguments == open else { return }
      Task { await controller.close() }
      // Once the close has run to its wait, a command sent the same way: a clearing sent meanwhile lands first.
      await afterQueuedWork()
      let probe: any CommandRunning = commands
      _ = await probe.run(["probe"], input: nil)
      overlap.seen = commands.ran.contains(closed)
    }
    controller.refresh()
    try await settle("the mark was never cleared") { overlap.seen != nil && commands.ran.contains(closed) }
    #expect(overlap.seen == false, "the window was marked closed while the open mark still ran")
    #expect(commands.ran.last == closed)
  }

  @Test("a blocked window marks itself open in the helper, where the opener waits for it, and not in your account")
  func aBlockedWindowMarksItselfOnlyInTheHelper() async throws {
    let open = Action.markWindow(open: true).arguments
    let unsigned = Self.helperReply(accessibility: false)
      .replacingOccurrences(of: #""signed": true"#, with: #""signed": false"#)
    let helper = ScriptedCommands(["onboarding status": unsigned, "onboarding window": #"{"ok": true}"#])
    let blocked = try await Self.loaded(helper)
    guard case .blocked = blocked.snapshot.step else {
      Issue.record("an unsigned Atlas in the helper is blocked")
      return
    }
    try await settle("the blocked window was never marked") { helper.ran.contains(open) }

    let mine = freshMac.replacingOccurrences(of: #""signed": true"#, with: #""signed": false"#)
    let person = ScriptedCommands(["onboarding status": mine, "onboarding window": #"{"ok": true}"#])
    let yours = try await Self.loaded(person)
    guard case .blocked = yours.snapshot.step else {
      Issue.record("an unsigned Atlas in your account is blocked")
      return
    }
    // A second reading: a mark the first one started would have reached the command line before it.
    yours.refresh()
    try await settle("the facts were never read again") {
      person.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 2
    }
    #expect(!person.ran.contains(open))
  }

  @Test("a check that ends once the window has closed changes nothing")
  func aLateCheckIsDropped() async throws {
    let commands = ScriptedCommands(["onboarding status": checkingMac, "onboarding finish": #"{"ok": true}"#])
    let controller = Self.controller(commands)
    commands.whileRunning { arguments in
      if arguments == Action.finishSetup.arguments { await controller.close() }
    }
    controller.refresh()
    try await settle("the check never ran") { commands.ran.contains(Action.finishSetup.arguments) }
    // The check's own end is the sentinel: once it has ended, it has done all it ever will.
    let check = try #require(controller.finishing, "the check is under way")
    try await check.value("the check never ended")
    #expect(controller.snapshot.finish == .running, "the closed window took the check's outcome")
  }

  @Test("the check runs once at a time: Try Again while it runs starts no second one")
  func theCheckRunsOnceAtATime() async throws {
    let commands = ScriptedCommands([
      "onboarding status": checkingMac, "onboarding finish": #"{"ok": false, "error": "The helper did not answer"}"#,
    ])
    let controller = Self.controller(commands)
    commands.whileRunning { arguments in
      if arguments == Action.finishSetup.arguments { controller.send(.retryFinish) }
    }
    controller.refresh()
    try await settle("the check never ended") { controller.snapshot.finish == .failed("The helper did not answer") }
    #expect(commands.ran.filter { $0 == Action.finishSetup.arguments }.count == 1)
    await controller.close()
  }

  @Test("agents read once the window has closed change nothing")
  func lateAgentsAreDropped() async throws {
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
    ])
    let controller = Self.controller(commands)
    commands.whileRunning { arguments in
      if arguments == Action.agentsStatus.arguments { await controller.close() }
    }
    controller.refresh()
    try await settle("the agents were never asked for") { commands.ran.contains(Action.agentsStatus.arguments) }
    // The reading's own end is the sentinel: once it has ended, it has done all it ever will.
    let reading = try #require(controller.loadingAgents, "the agents reading is under way")
    try await reading.value("the agents reading never ended")
    #expect(!controller.snapshot.agentsLoaded, "the closed window took the agents' reply")
  }

  @Test("a failed first reading's message goes once a reading succeeds")
  func aFailedFirstReadingIsForgotten() async throws {
    let commands = ScriptedCommands([:])
    let controller = Self.controller(commands)
    controller.refresh()
    try await settle("the failed reading was never shown") { controller.snapshot.problem != nil }
    commands.answer("onboarding status", freshMac)
    controller.refresh()
    try await settle("the facts were never read") { controller.snapshot.loaded }
    #expect(controller.snapshot.problem == nil)
  }

  @Test(
    "a failed agents reading keeps the agents listed unchecked, lets Continue skip the step, and is asked again later"
  )
  func aFailedAgentsReadingIsAskedAgainLater() async throws {
    let commands = ScriptedCommands(["onboarding status": finishedMac])
    let controller = try await Self.loaded(commands)
    try await settle("the failed agents reading was never shown") { controller.snapshot.agentsFailed }
    #expect(controller.snapshot.problem == OnboardingWords.agentsCheckFailed)
    #expect(controller.snapshot.agents.map(\.id) == AgentID.allCases, "the agents stay listed")
    #expect(controller.state.footer.next?.isEnabled == true, "Continue skips the step")
    controller.refresh()
    try await settle("the next reading never ran") {
      commands.ran.count { $0 == ["onboarding", "status"] } >= 2
    }
    #expect(
      commands.ran.count { $0 == ["agents", "status"] } == 1,
      "not asked again with the very next reading"
    )
    commands.answer(
      "agents status",
      #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#
    )
    try await settle("the agents were never asked for again") {
      controller.refresh()
      return controller.snapshot.agentsLoaded
    }
    #expect(controller.snapshot.agents.first { $0.id == .claude }?.found == true)
    #expect(controller.snapshot.problem == nil)
  }

  @Test("leaving the check and coming back runs it again")
  func theCheckRunsAgainOnReturn() async throws {
    let shown = checkingMac.replacingOccurrences(of: #""hidden": true"#, with: #""hidden": false"#)
    let commands = ScriptedCommands([
      "onboarding status": checkingMac, "onboarding finish": #"{"ok": false, "error": "no answer"}"#,
    ])
    let controller = try await Self.loaded(commands)
    try await settle("the check never failed") {
      controller.snapshot.finish != .idle && controller.snapshot.finish != .running
    }
    commands.answer("onboarding status", shown)
    controller.refresh()
    try await settle("the check was never left") { controller.snapshot.step == .switchToHelper }
    commands.answer("onboarding status", checkingMac)
    controller.refresh()
    try await settle("the check never ran again") {
      commands.ran.filter { $0 == Action.finishSetup.arguments }.count == 2
    }
  }

  @Test("agents that cannot be connected are each named with why, and switching them off moves on without the note")
  func agentsThatCannotBeConnectedAreNamed() async throws {
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": """
      {"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false},
                              {"agent": "codex", "installed": true, "connected": false}]}
      """,
      "agents install": """
      {"ok": false, "agents": [{"agent": "claude", "ok": true, "connected": true},
                               {"agent": "codex", "ok": false, "error": "codex plugin install failed"}]}
      """,
    ])
    let controller = try await Self.loaded(commands)
    try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
    await controller.connectAgents()
    #expect(commands.ran.contains(Action.installAgents([.claude, .codex]).arguments))
    #expect(controller.snapshot.problem == OnboardingWords.notConnected(.codex, "codex plugin install failed"))
    #expect(controller.snapshot.agents.first { $0.id == .claude }?.connected == true)
    #expect(!controller.snapshot.choices.agentsDone)
    // Codex switched off, Continue moves on without the failed install's note.
    controller.send(.setAgent(id: "codex", on: false))
    await controller.connectAgents()
    #expect(commands.ran.count { $0.starts(with: ["agents", "install"]) } == 1)
    #expect(controller.snapshot.problem == nil)
    #expect(controller.snapshot.choices.agentsDone)
  }

  @Test("Open at Login starts from what the login item says once the person has chosen")
  func openAtLoginStartsFromTheEarlierChoice() async throws {
    let decided = freshMac.replacingOccurrences(
      of: #""configured": false"#,
      with: #""menubarLogin": true, "configured": false"#
    )
    let controller = try await Self.loaded(ScriptedCommands(["onboarding status": decided]))
    #expect(!controller.snapshot.opensAtLogin, "the login item is off, as the person left it")
  }

  @Test("the agents step lists every agent with what atlas agents status reports")
  func theAgentsAreLoaded() async throws {
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
    ])
    let controller = try await Self.loaded(commands)
    try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
    #expect(controller.snapshot.agents.map(\.id) == AgentID.allCases)
    #expect(controller.snapshot.agents.first?.on == true, "a found agent is switched on")
  }

  @Test("an agents install Settings started holds the setup window's Continue, and the two never install at once")
  func installsFromBothWindowsNeverOverlap() async throws {
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
      "agents install": #"{"ok": true}"#,
    ])
    let gate = ReleaseSignal()
    commands.whileRunning { arguments in
      if arguments.prefix(2) == ["agents", "install"] { await gate.wait() }
    }
    let installs = AgentInstalls(commands: commands)
    let onboarding = try await Self.loaded(commands, installs: installs)
    try await settle("the agents were never read") { onboarding.snapshot.agentsLoaded }
    let settings = SettingsController.scripted(commands: commands, installs: installs)
    let connecting = Task { await settings.connect("codex") }
    try await settle("Settings' Connect never started") { installs.isBusy }
    #expect(onboarding.state.footer.next?.isEnabled == false, "Continue waits for the other window's install")
    await onboarding.connectAgents()
    #expect(onboarding.snapshot.problem == HelperWords.waitForAction)
    gate.open()
    await connecting.value
    let installed = commands.ran.filter { $0.prefix(2) == ["agents", "install"] }
    #expect(installed == [Action.installAgents([.codex]).arguments])
  }

  @Test("Continue after a skipped agents check clears its note and moves on to Done, connecting nothing")
  func continueAfterASkippedCheckClearsItsNote() async throws {
    let commands = ScriptedCommands(["onboarding status": finishedMac])
    let controller = try await Self.loaded(commands)
    try await settle("the failed agents reading was never shown") { controller.snapshot.agentsFailed }
    #expect(controller.snapshot.problem == OnboardingWords.agentsCheckFailed)
    await controller.connectAgents()
    #expect(!commands.ran.contains { $0.starts(with: ["agents", "install"]) })
    #expect(controller.snapshot.problem == nil)
    #expect(controller.snapshot.choices.agentsDone)
  }

  @Test("Continue connects the chosen agents and moves on to Done")
  func continueConnectsTheAgents() async throws {
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
      "agents install": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands)
    try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
    await controller.connectAgents()
    #expect(commands.ran.contains(Action.installAgents([.claude]).arguments))
    #expect(controller.snapshot.agents.first?.connected == true)
    #expect(controller.snapshot.problem == nil)
    #expect(controller.snapshot.choices.agentsDone, "the window moves on to Done")
  }

  @Test("a failed install connects no agent and stays on the agents, saying why")
  func aFailedInstallStops() async throws {
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
      "agents install": #"{"ok": false, "error": "npm is missing"}"#,
    ])
    let controller = try await Self.loaded(commands)
    try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
    await controller.connectAgents()
    #expect(commands.ran.contains(Action.installAgents([.claude]).arguments))
    #expect(controller.snapshot.agents.allSatisfy { !$0.connected })
    #expect(!controller.snapshot.choices.agentsDone)
    #expect(controller.snapshot.problem == "npm is missing.")
  }

  @Test("Done sets Open at Login and opens Atlas, then the window closes")
  func doneClosesTheWindow() async {
    let commands = ScriptedCommands(["onboarding login": #"{"ok": true}"#, "menubar": #"{"ok": true}"#])
    let controller = Self.controller(commands)
    controller.send(.setOpenAtLogin(false))
    await controller.complete()
    let done = commands.ran.filter { $0 != Action.onboardingStatus.arguments }
    #expect(done == [Action.setOpensAtLogin(false).arguments, Action.openMenuBar.arguments])
    #expect(controller.closeRequested)
  }

  @Test("Done that fails keeps the window open, saying why")
  func aFailedDoneKeepsTheWindow() async {
    let controller = Self.controller(ScriptedCommands(["onboarding login": #"{"ok": true}"#]))
    await controller.complete()
    #expect(!controller.closeRequested)
    #expect(controller.snapshot.problem == "unknown command.")
  }

  @Test(
    "switching back locks for the account that asked, or with none named when no one did",
    arguments: [("me", Action.switchTo("me")), (nil, .lockScreen)] as [(String?, Action)]
  )
  func switchingBack(asker: String?, action: Action) async throws {
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(asker: asker, accessibility: true), "onboarding window": #"{"ok": true}"#,
      "switch": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands)
    await controller.switchBack()
    #expect(commands.ran.contains(action.arguments))
    #expect(controller.closeRequested)
  }

  @Test("the helper allows exactly the account it showed")
  func theHelperAllowsTheAccountShown() async throws {
    let commands = ScriptedCommands([
      "onboarding status": """
      {"ok": true, "role": "helper", "me": {"name": "atlas"}, "app": {"installed": true, "signed": true},
       "helper": {"controller": {"uid": 501, "name": "me"}, "installed": false, "running": false}}
      """,
      "onboarding accept": #"{"ok": true}"#,
      "onboarding window": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands)
    #expect(controller.snapshot.step == .allow)
    await controller.allow()
    #expect(commands.ran.contains(Action.allow(uid: 501, name: "me").arguments))
  }

  @Test("a permission's button opens its own Privacy & Security page, with the panel beside it")
  func allowingAPermissionOpensItsPage() async throws {
    let opened = OpenedURLs()
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = Self.controller(commands, opened: opened)
    controller.refresh()
    try await settle("the facts were never read") { controller.snapshot.loaded }
    controller.send(.openSettings(id: Permission.screenRecording.id))
    #expect(opened.urls == [Permission.screenRecording.settingsURL].compactMap { $0 })
    #expect(controller.snapshot.guiding == .screenRecording)
  }

  @Test("the setup window reads again after its interval, and not before")
  func theWindowReadsAgainAfterItsInterval() async throws {
    let clock = ManualClock()
    let commands = ScriptedCommands(["onboarding status": freshMac])
    let controller = Self.controller(commands, clock: clock)
    controller.watch()
    try await clock.untilSleeping(for: OnboardingController.readEvery, "the first reading never paused")
    #expect(commands.ran == [Action.onboardingStatus.arguments])
    #expect(clock.waiting == [OnboardingController.readEvery])
    clock.advance(by: OnboardingController.readEvery - .milliseconds(1))
    #expect(clock.waiting == [.milliseconds(1)], "the pause has not ended a moment before its interval")
    clock.advance(by: .milliseconds(1))
    try await settle("the facts were never read again") { commands.ran.count == 2 }
    controller.stop()
  }

  @Test("the panel opens the next permission's page once its switch is on, and steps aside once both are")
  func thePanelMovesOn() async throws {
    let opened = OpenedURLs()
    let clock = ManualClock()
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = Self.controller(commands, opened: opened, clock: clock)
    controller.refresh()
    try await settle("the facts were never read") { controller.snapshot.loaded }
    controller.send(.openSettings(id: Permission.accessibility.id))
    controller.refresh()
    try await settle("the facts were never read again") {
      commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 2
    }
    #expect(controller.snapshot.guiding == .accessibility, "still waiting for the switch")
    commands.answer("onboarding status", Self.helperReply(accessibility: true))
    controller.refresh()
    try await clock.untilSleeping(for: OnboardingController.tickShown, "Accessibility's tick never showed")
    clock.advance(by: OnboardingController.tickShown)
    try await settle("the panel never moved on") { controller.snapshot.guiding == .screenRecording }
    #expect(opened.urls == [Permission.accessibility, .screenRecording].compactMap(\.settingsURL))
    commands.answer("onboarding status", Self.helperReply(accessibility: true, screenCapture: true))
    controller.refresh()
    try await clock.untilSleeping(for: OnboardingController.tickShown, "Screen Recording's tick never showed")
    clock.advance(by: OnboardingController.tickShown)
    try await settle("the panel never stepped aside") { controller.snapshot.guiding == nil }
    #expect(controller.snapshot.step == .helperDone)
    #expect(opened.urls.count == 2, "each page opens once")
  }

  @Test(
    "a tick under way moves on once however often the facts are read, and Back stops it even if the panel opens again"
  )
  func aTickUnderWay() async throws {
    let opened = OpenedURLs()
    let clock = ManualClock()
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = Self.controller(commands, opened: opened, clock: clock)
    controller.refresh()
    try await settle("the facts were never read") { controller.snapshot.loaded }
    controller.send(.openSettings(id: Permission.accessibility.id))
    commands.answer("onboarding status", Self.helperReply(accessibility: true))
    controller.refresh()
    try await clock.untilSleeping(for: OnboardingController.tickShown, "the tick never started")
    controller.refresh()
    try await settle("the facts were never read again") {
      commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 3
    }
    #expect(clock.waiting == [OnboardingController.tickShown], "one tick however often the facts are read")
    let tick = try #require(controller.movingOn, "the tick is under way")
    controller.send(.closePanel)
    controller.send(.openSettings(id: Permission.accessibility.id))
    #expect(clock.waiting.isEmpty, "Back ended the tick's wait")
    // The tick's own end is the sentinel: once it has ended, it has done all it ever will.
    try await tick.value("the stopped tick never ended")
    let accessibility = try #require(Permission.accessibility.settingsURL)
    #expect(opened.urls == [accessibility, accessibility], "Screen Recording's page never opened")
    #expect(controller.snapshot.guiding == .accessibility)
  }

  @Test("Back closes the panel and stops it moving on")
  func backClosesThePanel() async throws {
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands)
    controller.send(.openSettings(id: Permission.accessibility.id))
    controller.send(.closePanel)
    #expect(controller.snapshot.guiding == nil)
    #expect(controller.snapshot.step == .permissions)
  }

  @Test("a job restarting to pick up Screen Recording keeps its permissions and the permissions screen")
  func aRestartingJobKeepsItsPermissions() async throws {
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(accessibility: true), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands)
    commands.answer("onboarding status", Self.restartingReply)
    controller.refresh()
    try await settle("the facts were never read again") {
      commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 2
    }
    // The reading answers a turn after it is asked, on the main actor: behind it, its facts have landed.
    await afterQueuedWork()
    #expect(controller.snapshot.step == .permissions, "not Allow again")
    #expect(controller.snapshot.facts.asker.accessibility, "not off")
  }

  @Test("a job that answers nothing for too long counts as stopped")
  func aStoppedJobAsksForAllowAgain() async throws {
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(accessibility: true), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands)
    commands.answer("onboarding status", Self.restartingReply)
    for reading in 2...(OnboardingController.restartReadings + 2) {
      controller.refresh()
      try await settle("the facts were never read again") {
        commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == reading
      }
    }
    try await settle("the job never counted as stopped") { controller.snapshot.step == .allow }
  }
}
