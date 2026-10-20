import AtlasKit
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// A fresh Mac's reply: no helper chosen, and no standard account that could be one.
private let freshMac = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": false, "candidates": []}
  """

/// A set-up Mac's reply: the helper `atlas` is chosen, allowed and checked, so the agents come next.
private let finishedMac = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": true, "finished": true, "candidates": [],
   "helper": {"exists": true, "name": "atlas", "administrator": false, "paired": true, "signedIn": true,
              "answering": true, "hidden": true, "permissions": {"screenCapture": true, "accessibility": true}}}
  """

/// A Mac whose helper is chosen, allowed and answering, but not checked yet: the end-to-end check runs next.
private let checkingMac = finishedMac.replacingOccurrences(of: #""finished": true"#, with: #""finished": false"#)

@MainActor
@Suite("OnboardingController")
struct OnboardingControllerTests {
  /// A controller that runs `commands` and reaches nothing else of the system.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line.
  ///   - opened: Records the URLs opened.
  ///   - folders: What the folder chooser answers.
  ///   - loginItem: The scripted login item.
  /// - Returns: The controller.
  private static func controller(
    _ commands: ScriptedCommands,
    opened: OpenedURLs = OpenedURLs(),
    folders: [URL] = [],
    loginItem: ScriptedLoginItem = ScriptedLoginItem()
  ) -> OnboardingController {
    OnboardingController(
      dependencies: OnboardingDependencies(
        commands: commands,
        openURL: { opened.urls.append($0) },
        chooseFolders: { folders },
        loginItem: loginItem.seam
      )
    )
  }

  /// A controller that has read `reply` once.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line, answering `onboarding status`.
  ///   - folders: What the folder chooser answers.
  /// - Returns: The controller, loaded.
  /// - Throws: The failed requirement, when the facts are never read.
  private static func loaded(_ commands: ScriptedCommands, folders: [URL] = []) async throws -> OnboardingController {
    let controller = controller(commands, folders: folders)
    controller.refresh()
    try await settle("the facts were never read") { controller.snapshot.loaded }
    return controller
  }

  /// A helper-side reply for the account `me`, with Atlas running and its two permissions as given.
  ///
  /// - Parameters:
  ///   - asker: The account that asked, or nil when no one did.
  ///   - accessibility: Whether Accessibility is on.
  /// - Returns: The reply.
  private static func helperReply(asker: String? = "me", accessibility: Bool) -> String {
    let controller = asker.map { #"{"uid": 501, "name": "\#($0)"}"# } ?? "null"
    return """
      {"ok": true, "role": "helper", "me": {"name": "atlas"}, "app": {"installed": true, "signed": true},
       "helper": {"controller": \(controller), "installed": true, "running": true,
                  "permissions": {"accessibility": \(accessibility), "screenCapture": false}}}
      """
  }

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
  func aFailedCommandIsShown() async {
    let controller = Self.controller(ScriptedCommands(["onboarding status": freshMac]))
    controller.send(.pick(.existing("qa")))
    await controller.useHelper()
    #expect(controller.snapshot.problem == "unknown command")
    #expect(controller.snapshot.working == nil)
  }

  @Test("a closed window starts no reading")
  func aClosedWindowStartsNothing() async {
    let commands = ScriptedCommands(["onboarding status": freshMac])
    let controller = Self.controller(commands)
    await controller.close()
    controller.refresh()
    // Proves an absence: a reading would have started well within this pause.
    try? await Task.sleep(for: Ceiling.absence)
    #expect(commands.ran.isEmpty)
    #expect(!controller.snapshot.loaded)
  }

  @Test("closing setup while a command runs lets it settle and starts no further command")
  func closingWhileACommandRunsStartsNoMore() async throws {
    let opie = URL(fileURLWithPath: "/Users/me/dev/Opie")
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
      "agents install": #"{"ok": true}"#,
      "agents project": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands, folders: [opie])
    try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
    controller.send(.chooseProject)
    // The window closes while the agents are installed, before the project would be taught.
    commands.whileRunning { arguments in
      if arguments.starts(with: ["agents", "install"]) { await controller.close() }
    }
    await controller.connectAgents()
    #expect(commands.ran.contains(Action.installAgents([.claude]).arguments), "the running command settled")
    #expect(!commands.ran.contains(Action.teachProject(path: opie.path).arguments))
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
    var overlapped: Bool?
    // The person closes the window while the open mark runs.
    commands.whileRunning { arguments in
      guard arguments == open else { return }
      Task { await controller.close() }
      try? await Task.sleep(for: Ceiling.absence)
      overlapped = commands.ran.contains(closed)
    }
    controller.refresh()
    try await settle("the mark was never cleared") { overlapped != nil && commands.ran.contains(closed) }
    #expect(overlapped == false, "the window was marked closed while the open mark still ran")
    #expect(commands.ran.last == closed)
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
    try? await Task.sleep(for: Ceiling.absence)
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
    try? await Task.sleep(for: Ceiling.absence)
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

  @Test("a failed agents reading says why and is asked again with the next reading")
  func aFailedAgentsReadingIsAskedAgain() async throws {
    let commands = ScriptedCommands(["onboarding status": finishedMac])
    let controller = try await Self.loaded(commands)
    try await settle("the failed agents reading was never shown") { controller.snapshot.problem != nil }
    #expect(!controller.snapshot.agentsLoaded, "nothing is known about the agents")
    commands.answer(
      "agents status",
      #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#
    )
    controller.refresh()
    try await settle("the agents were never asked for again") { controller.snapshot.agentsLoaded }
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

  @Test("agents that cannot be connected are each named with why, and the window stays")
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

  @Test("a chosen folder is added once, however it is spelled or linked to, and removed by its path")
  func chosenFoldersAreAddedOnce() throws {
    let files = FileManager.default
    let home = files.temporaryDirectory
      .appendingPathComponent("atlas-projects-\(UUID().uuidString)")
    let opie = home.appendingPathComponent("Opie", isDirectory: true)
    try files.createDirectory(at: opie, withIntermediateDirectories: true)
    defer { try? files.removeItem(at: home) }
    let link = home.appendingPathComponent("Opie link")
    try files.createSymbolicLink(at: link, withDestinationURL: opie)
    let spellings = [opie, URL(fileURLWithPath: home.path + "/./Opie"), link]
    let controller = Self.controller(ScriptedCommands([:]), folders: spellings)
    controller.send(.chooseProject)
    controller.send(.chooseProject)
    let chosen = try #require(controller.snapshot.projects.first)
    #expect(controller.snapshot.projects.count == 1)
    controller.send(.removeProject(id: chosen.path))
    #expect(controller.snapshot.projects.isEmpty)
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

  @Test("Continue connects the chosen agents, then teaches each project, naming one that cannot be taught")
  func continueConnectsAgentsAndTeachesProjects() async throws {
    let opie = URL(fileURLWithPath: "/Users/me/dev/Opie")
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
      "agents install": #"{"ok": true}"#,
      "agents project": #"{"ok": false, "error": "No AGENTS.md here"}"#,
    ])
    let controller = try await Self.loaded(commands, folders: [opie])
    try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
    controller.send(.chooseProject)
    await controller.connectAgents()
    #expect(commands.ran.contains(Action.installAgents([.claude]).arguments))
    #expect(commands.ran.contains(Action.teachProject(path: opie.path).arguments))
    #expect(controller.snapshot.agents.first?.connected == true)
    #expect(controller.snapshot.taught.isEmpty)
    #expect(controller.snapshot.problem == OnboardingWords.notTaught("Opie", "No AGENTS.md here"))
    #expect(controller.snapshot.choices.agentsDone, "the window moves on to Done")
  }

  @Test("a failed install teaches no project, connects no agent and stays on the agents, saying why")
  func aFailedInstallStops() async throws {
    let opie = URL(fileURLWithPath: "/Users/me/dev/Opie")
    let commands = ScriptedCommands([
      "onboarding status": finishedMac,
      "agents status": #"{"ok": true, "agents": [{"agent": "claude", "installed": true, "connected": false}]}"#,
      "agents install": #"{"ok": false, "error": "npm is missing"}"#,
      "agents project": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands, folders: [opie])
    try await settle("the agents were never read") { controller.snapshot.agentsLoaded }
    controller.send(.chooseProject)
    await controller.connectAgents()
    #expect(commands.ran.contains(Action.installAgents([.claude]).arguments))
    #expect(!commands.ran.contains { $0.starts(with: ["agents", "project"]) })
    #expect(controller.snapshot.agents.allSatisfy { !$0.connected })
    #expect(!controller.snapshot.choices.agentsDone)
    #expect(controller.snapshot.problem == "npm is missing")
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
    #expect(controller.snapshot.problem == "unknown command")
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

  @Test("a permission's button opens its own Privacy & Security page, with the guide beside it")
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

  @Test("the guide panel steps aside once its switch is on")
  func theGuideStepsAside() async throws {
    let commands = ScriptedCommands([
      "onboarding status": Self.helperReply(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = try await Self.loaded(commands)
    controller.send(.openSettings(id: Permission.accessibility.id))
    controller.refresh()
    try await settle("the facts were never read again") {
      commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 2
    }
    #expect(controller.snapshot.guiding == .accessibility, "still waiting for the switch")
    commands.answer("onboarding status", Self.helperReply(accessibility: true))
    controller.refresh()
    try await settle("the guide never stepped aside") { controller.snapshot.guiding == nil }
  }
}
