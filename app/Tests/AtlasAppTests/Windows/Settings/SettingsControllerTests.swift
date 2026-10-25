import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import CoreGraphics
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("SettingsController")
struct SettingsControllerTests {
  @Test("a failed project refresh keeps the last loaded list and its count on the detail page")
  func failedProjectRefreshKeepsTheList() async {
    let commands = ScriptedCommands(["agents status": Self.statusWithOpie])
    let controller = SettingsController.scripted(commands: commands)
    controller.send(.navigate(.projects))
    await controller.agentsReading?.value
    let projects = controller.state.agents.projects
    commands.answer("agents status", #"{"ok":false,"error":"Cannot read agents"}"#)
    await controller.loadAgents()
    #expect(controller.agents.failed)
    #expect(controller.state.location == .projects)
    #expect(controller.state.agents.projects == projects)
    #expect(controller.state.agents.projectsSummary == "1 project taught")
  }

  @Test("relocated field edits read fresh shared values and call only their original writers")
  func independentPreferenceEdits() async throws {
    let commands = ScriptedCommands([:])
    var dependencies = SettingsController.scripted(commands: commands).dependencies
    var picture = ViewerPictureChoice.live
    var startup = false
    var pictureWrites: [ViewerPictureChoice] = []
    var startupWrites: [Bool] = []
    let startupCalls = CallCount()
    var notifyWrites: [Bool] = []
    dependencies.links.picture = { picture }
    dependencies.links.opensAtLogin = { OpenAtLogin.State(isAvailable: true, isEnabled: startup) }
    dependencies.links.choosePicture = {
      picture = $0
      pictureWrites.append($0)
    }
    dependencies.links.setOpenAtLogin = {
      startup = $0
      startupWrites.append($0)
      startupCalls.count += 1
    }
    dependencies.preferences.saveNotify = { notifyWrites.append($0) }
    let controller = SettingsController(dependencies: dependencies)
    // The viewer and panel change after a previous Settings snapshot was drawn.
    _ = controller.state
    picture = .everyFiveSeconds
    startup = true
    controller.send(.setNotify(true))
    #expect(pictureWrites.isEmpty && startupWrites.isEmpty)
    #expect(controller.state.general.picture == .everyFiveSeconds && controller.state.general.opensAtLogin)
    controller.send(.choosePicture(.live))
    #expect(pictureWrites == [.live] && startupWrites.isEmpty && notifyWrites == [true])
    controller.send(.setOpenAtLogin(false))
    try await settle("the original startup writer never ran") { startupCalls.count == 1 }
    #expect(startupWrites == [false])
    controller.send(.setNotify(false))
    #expect(notifyWrites == [true, false])
    #expect(pictureWrites == [.live] && !startup)
    #expect(commands.ran.isEmpty)
  }

  @Test("new roots restore directly and details save only their owning root")
  func rootsAndDetailsRestore() async {
    for root in [SettingsRoot.screen, .notifications] {
      let commands = ScriptedCommands([:])
      var dependencies = SettingsController.scripted(commands: commands).dependencies
      dependencies.preferences.loadTab = { root.rawValue }
      let controller = SettingsController(dependencies: dependencies)
      #expect(controller.state.location == .root(root))
      #expect(commands.ran.isEmpty)
    }
    let commands = ScriptedCommands(["agents status": Self.statusWithOpie])
    var dependencies = SettingsController.scripted(commands: commands).dependencies
    var saved: [String] = []
    dependencies.preferences.saveTab = { saved.append($0) }
    let controller = SettingsController(dependencies: dependencies)
    controller.send(.navigate(.diagnostics))
    #expect(controller.state.title == "Readiness & Diagnostics")
    controller.send(.navigate(.projects))
    await controller.agentsReading?.value
    #expect(saved == ["helper", "agents"])
    #expect(controller.state.title == "Projects")
    controller.close()
    #expect(controller.state.location == .root(.agents))
    #expect(commands.ran == [Action.agentsStatus.arguments])
  }

  @Test("Copy Diagnostics delegates once to the existing link without running another Settings command")
  func diagnosticsDelegates() {
    let commands = ScriptedCommands([:])
    var dependencies = SettingsController.scripted(commands: commands).dependencies
    var copies = 0
    dependencies.links.copyDiagnostics = { copies += 1 }
    let controller = SettingsController(dependencies: dependencies)
    controller.send(.navigate(.diagnostics))
    #expect(copies == 0)
    controller.send(.copyDiagnostics)
    #expect(copies == 1 && commands.ran.isEmpty)
  }

  @Test("a check remains shared across Helper and Diagnostics and keeps its diagnosis on either route")
  func diagnosticsSharesChecking() async throws {
    let commands = ScriptedCommands(["doctor": #"{"ok":true,"ready":false,"nextSteps":["Check the helper."]}"#])
    let clock = ManualClock()
    let controller = SettingsController.scripted(commands: commands, clock: clock)
    controller.send(.navigate(.root(.helper)))
    let checking = Task { await controller.checkAgain() }
    let least = Duration.seconds(Motion.Duration.leastWork.seconds)
    try await clock.untilSleeping(for: least, "the check never held")
    controller.send(.navigate(.diagnostics))
    #expect(controller.state.helper.readinessSummary == "Checking…")
    await controller.checkAgain()
    clock.advance(by: least)
    await checking.value
    let diagnosis = controller.state.helper.readyNote
    #expect(diagnosis?.text == "Check the helper.")
    controller.send(.back)
    #expect(controller.state.helper.readyNote == diagnosis)
    #expect(commands.ran == [Action.doctor.arguments])
  }

  @Test("a failed Forget leaves the path and failure visible through Projects and its owning Agents root")
  func failedForgetPersistsAcrossRoutes() async {
    let commands = ScriptedCommands([
      "agents status": Self.statusWithOpie,
      "agents forget": #"{"ok":false,"error":"Cannot write AGENTS.md"}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    controller.send(.navigate(.projects))
    await controller.agentsReading?.value
    controller.send(.askForget(Self.opie))
    #expect(controller.session.alert == .forget(Self.opie))
    await controller.confirmAlert()
    let failure = controller.state.agents.notice
    #expect(failure?.isProblem == true)
    #expect(controller.state.agents.projects.map(\.id) == [Self.opie])
    controller.send(.navigate(.root(.agents)))
    await controller.agentsReading?.value
    #expect(controller.state.agents.notice == failure)
    controller.send(.back)
    await controller.agentsReading?.value
    #expect(controller.state.agents.notice == failure)
    #expect(commands.ran.filter { $0 == Action.forgetProject(path: Self.opie).arguments }.count == 1)
  }

  @Test("Back and Forward into Agents refresh projects with only a status read and remember the destination")
  func historyRefreshesAgents() async {
    let commands = ScriptedCommands(["agents status": #"{"ok": true, "agents": [], "projects": []}"#])
    var saved: String?
    var title: String?
    var dependencies = SettingsController.scripted(commands: commands).dependencies
    dependencies.preferences.saveTab = { saved = $0 }
    let controller = SettingsController(dependencies: dependencies)
    controller.nameWindow = { title = $0 }
    controller.send(.selectTab(.agents))
    await controller.agentsReading?.value
    controller.send(.selectTab(.about))
    commands.answer("agents status", Self.statusWithOpie)
    controller.send(.back)
    await controller.agentsReading?.value
    #expect(controller.agents.projects.map(\.path) == [Self.opie])
    #expect(saved == "agents")
    #expect(title == "Settings — Agents")
    controller.send(.back)
    commands.answer("agents status", #"{"ok": true, "agents": [], "projects": []}"#)
    controller.send(.forward)
    await controller.agentsReading?.value
    #expect(controller.agents.projects.isEmpty)
    #expect(saved == "agents")
    #expect(title == "Settings — Agents")
    #expect(commands.ran == Array(repeating: Action.agentsStatus.arguments, count: 3))
    controller.close()
  }

  @Test("history navigation preserves active work and alerts without running helper or update commands")
  func navigationPreservesWorkAndAlerts() {
    let commands = ScriptedCommands([:])
    let controller = SettingsController.scripted(commands: commands)
    controller.send(.askRecover)
    controller.session.checking = true
    controller.session.update = .checking
    controller.session.alertRunning = true
    controller.session.alertRefusal = "Still waiting"
    let before = controller.session
    controller.send(.selectTab(.helper))
    controller.send(.selectTab(.about))
    controller.send(.back)
    controller.send(.forward)
    var expected = before
    expected.navigation = controller.session.navigation
    #expect(controller.session == expected)
    #expect(commands.ran.isEmpty)
    #expect(controller.state.canGoBack && !controller.state.canGoForward)
  }

  @Test("closing and reopening keeps only the last root while preserving an update already running")
  func reopeningStartsAtTheRememberedRoot() {
    let commands = ScriptedCommands([:])
    var dependencies = SettingsController.scripted(commands: commands).dependencies
    var saved: String? = "helper"
    dependencies.preferences.loadTab = { saved }
    dependencies.preferences.saveTab = { saved = $0 }
    let controller = SettingsController(dependencies: dependencies)
    #expect(controller.session.tab == .helper)
    controller.send(.selectTab(.about))
    controller.send(.back)
    #expect(saved == "helper")
    controller.send(.forward)
    #expect(saved == "about")
    controller.session.update = .checking
    controller.close()
    #expect(controller.session.navigation.entries == [.root(.about)])
    #expect(controller.session.update == .checking)
    let reopened = SettingsController(dependencies: dependencies)
    #expect(reopened.session.tab == .about)
    #expect(!reopened.state.canGoBack && !reopened.state.canGoForward)
    #expect(commands.ran.isEmpty)
  }

  /// A project `atlas agents status` lists as taught.
  static let opie = "/Users/me/dev/opie"
  /// `atlas agents status` listing ``opie``.
  static let statusWithOpie =
    #"{"ok": true, "agents": [], "projects": [{"path": "/Users/me/dev/opie", "taught": true}]}"#

  @Test("Check for Updates never runs twice at once, even across closing and reopening the window")
  func checkForUpdatesRunsOnce() async throws {
    let commands = ScriptedCommands(["update": #"{"ok": true, "version": "0.1.0", "latest": "0.1.0", "newer": false}"#])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let controller = SettingsController.scripted(commands: commands)
    controller.send(.checkForUpdates)
    try await settle("the first check never started") { controller.session.update == .checking }
    // What closing the window and opening another does to the controller the app keeps.
    controller.close()
    controller.cameForward()
    await controller.checkForUpdates()
    gate.open()
    try await settle("the check never answered") { controller.session.update == .answered(.upToDate) }
    #expect(commands.ran.filter { $0.first == "update" } == [Action.checkForUpdate.arguments])
  }

  @Test("Forget asks before it takes the project's section out, then reads the projects again")
  func forgetAsksFirst() async {
    let commands = ScriptedCommands([
      "agents status": Self.statusWithOpie,
      "agents forget": #"{"ok": true, "files": [{"path": "/Users/me/dev/opie/AGENTS.md", "change": "removed"}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    controller.send(.askForget(Self.opie))
    #expect(controller.session.alert == .forget(Self.opie))
    #expect(!commands.ran.contains(Action.forgetProject(path: Self.opie).arguments), "nothing runs while it asks")
    await controller.confirmAlert()
    #expect(commands.ran.suffix(2) == [Action.forgetProject(path: Self.opie).arguments, Action.agentsStatus.arguments])
    #expect(controller.session.alert == nil)
  }

  @Test("a Forget that fails says why in the Agents pane")
  func aFailedForgetSaysWhy() async {
    let commands = ScriptedCommands([
      "agents status": Self.statusWithOpie,
      "agents forget": #"{"ok": false, "error": "AGENTS.md has an Atlas marker without its pair"}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    controller.send(.askForget(Self.opie))
    await controller.confirmAlert()
    let notice = controller.state.agents.notice
    #expect(notice?.text.contains("AGENTS.md has an Atlas marker without its pair") == true)
    #expect(notice?.isProblem == true, "drawn in the attention colour")
  }

  @Test("Connect on Claude connects the installed Claude app alone")
  func connectingTheClaudeAppNamesOnlyIt() async {
    let commands = ScriptedCommands([
      "agents install": #"{"ok": true, "agents": [{"agent": "claude-app", "ok": true}]}"#,
      "agents status": #"{"ok": true, "agents": [{"agent":"claude-app","installed":true,"connected":false}]}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.loadAgents()
    await controller.connect("claude")
    #expect(commands.ran.contains(["agents", "install", "--app"]))
  }

  @Test(
    "Close It closes the logins under macOS's approval, says what came of it, then reads the shared status again",
    arguments: [
      (#"{"ok": true, "closed": [502]}"#, SettingsNote(text: "Closed. You can go on.", kind: .quiet)),
      (
        #"{"ok": false, "code": "onboarding", "error": "It's still open. Close it before you switch."}"#,
        SettingsNote(text: "It's still open. Close it before you switch.", kind: .problem)
      ),
    ] as [(String, SettingsNote)]
  )
  func closeItSaysWhatCameOfIt(reply: String, note: SettingsNote) async {
    let commands = ScriptedCommands(["onboarding close": reply])
    let reads = CallCount()
    let controller = SettingsController.scripted(commands: commands, statusReads: reads)
    await controller.closeLogin(.closeLeftovers)
    #expect(commands.ran == [Action.closeLeftovers.arguments])
    #expect(controller.state.helper.notice == note)
    #expect(!controller.session.closing)
    #expect(reads.count == 1)
  }

  @Test(
    "Check Again diagnoses the helper, says what an unready one needs under Ready to act, then reads the shared status again"
  )
  func checkAgainDiagnoses() async throws {
    let commands = ScriptedCommands([
      "doctor": #"{"ok": true, "ready": false, "nextSteps": ["Start the helper: run atlas init (safe to run again)."]}"#
    ])
    let reads = CallCount()
    let clock = ManualClock()
    let controller = SettingsController.scripted(commands: commands, statusReads: reads, clock: clock)
    let checking = Task { await controller.checkAgain() }
    let least = Duration.seconds(Motion.Duration.leastWork.seconds)
    try await clock.untilSleeping(for: least, "Check Again never held for its least time")
    clock.advance(by: least)
    await checking.value
    #expect(commands.ran == [Action.doctor.arguments])
    #expect(
      controller.state.helper.readyNote
        == SettingsNote(text: "Start the helper: run atlas init (safe to run again).", kind: .problem)
    )
    #expect(controller.state.helper.notice == nil, "not under the last section")
    #expect(reads.count == 1)
  }

  @Test("Check Again shows for at least 0.6 seconds on the clock however fast it answers, and says nothing when ready")
  func checkAgainHoldsForItsLeastTime() async throws {
    let commands = ScriptedCommands(["doctor": #"{"ok": true, "ready": true}"#])
    let clock = ManualClock()
    let controller = SettingsController.scripted(commands: commands, clock: clock)
    let checking = Task { await controller.checkAgain() }
    let least = Duration.seconds(Motion.Duration.leastWork.seconds)
    try await clock.untilSleeping(for: least, "Check Again never held for its least time")
    try await settle("the diagnosis never ran") { commands.ran == [Action.doctor.arguments] }
    #expect(controller.session.checking, "answered at once, yet still checking")
    clock.advance(by: least)
    await checking.value
    #expect(!controller.session.checking)
    #expect(controller.state.helper.readyNote == nil && controller.state.helper.notice == nil, "the marks say it")
  }

  @Test("while Check Again reads, VoiceOver hears each check as Checking, never the result it is replacing")
  func checkAgainSpeaksChecking() async throws {
    let commands = ScriptedCommands(["doctor": #"{"ok": true, "ready": true}"#])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let checks: [[String: Any]] = [["name": "helperRunning", "ok": true], ["name": "inputReady", "ok": false]]
    let clock = ManualClock()
    let controller = SettingsController.scripted(
      commands: commands,
      clock: clock,
      status: try status(["checks": checks])
    )
    let before = controller.state.helper.ready
    try #require(!before.isEmpty, "the Helper pane lists its checks")
    #expect(!before.contains { $0.spokenValue == "Checking" })
    controller.send(.checkAgain)
    try await settle("Check Again never started") { controller.state.helper.ready.allSatisfy(\.isChecking) }
    #expect(controller.state.helper.ready.allSatisfy { $0.spokenValue == "Checking" })
    let least = Duration.seconds(Motion.Duration.leastWork.seconds)
    try await clock.untilSleeping(for: least, "Check Again never held for its least time")
    clock.advance(by: least)
    gate.open()
    try await settle("Check Again never ended") { !controller.state.helper.ready.contains(where: \.isChecking) }
    #expect(controller.state.helper.ready.map(\.spokenValue) == before.map(\.spokenValue), "the result is heard again")
  }

  @Test("Recover in Settings says the helper is ready for the hold on the clock, then closes and says what it did")
  func recoverHoldsThenCloses() async throws {
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let clock = ManualClock()
    let controller = SettingsController.scripted(commands: commands, clock: clock)
    controller.send(.askRecover)
    let confirming = Task { await controller.confirmAlert() }
    let hold = Duration.seconds(Motion.Duration.doneHold.seconds)
    try await clock.untilSleeping(for: hold, "Recover never held on its done words")
    #expect(
      controller.state.alert?.phase
        == .done(title: HelperWords.recovered, message: HelperWords.recoveredMessage, button: HelperWords.done)
    )
    clock.advance(by: hold)
    await confirming.value
    #expect(controller.session.alert == nil)
    #expect(controller.state.helper.notice == SettingsNote(text: Words.readyAgain, kind: .quiet))
  }

  @Test("Done in Settings closes a Recover that worked at once, without waiting for the hold, and says what it did")
  func doneClosesAtOnce() async throws {
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let clock = ManualClock()
    let controller = SettingsController.scripted(commands: commands, clock: clock)
    controller.send(.askRecover)
    let confirming = Task { await controller.confirmAlert() }
    let hold = Duration.seconds(Motion.Duration.doneHold.seconds)
    try await clock.untilSleeping(for: hold, "Recover never held on its done words")
    controller.send(.alertCancel)
    await confirming.value
    #expect(controller.session.alert == nil, "closed with the clock never advanced")
    #expect(controller.state.helper.notice == SettingsNote(text: Words.readyAgain, kind: .quiet))
  }

  @Test("a refused Recover in Settings keeps its alert, saying why, until Not Now")
  func aRefusedRecoverStays() async {
    let commands = ScriptedCommands(["start-input": #"{"ok": false, "code": "busy", "error": "busy"}"#])
    let controller = SettingsController.scripted(commands: commands)
    controller.send(.askRecover)
    await controller.confirmAlert()
    #expect(controller.state.alert?.phase == .refused(message: HelperWords.recoverBusy))
    #expect(controller.state.alert?.isBusy == false, "its buttons come back")
    controller.send(.alertCancel)
    #expect(controller.state.alert == nil)
  }

  @Test("a busy helper action makes the buttons wait and says why")
  func aBusyHelperActionMakesTheButtonsWait() async throws {
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let service = ScriptedService()
    let actions = HelperActions(
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    )
    let controller = SettingsController.scripted(commands: commands, actions: actions)
    #expect(controller.state.helper.waiting == nil)
    // Another window's Recover, through the commands every window shares.
    let elsewhere = Task { await actions.recover() }
    try await settle("the other Recover never ran") { commands.ran.contains(Action.startInput.arguments) }
    let helper = controller.state.helper
    #expect(helper.waiting == "Recover is running. Try again once it ends.", "a line with a spinner names it")
    #expect(helper.notice == nil, "not an orange sentence")
    #expect(!helper.buttons.canSwitch)
    #expect(!helper.buttons.canRecover)
    #expect(!helper.buttons.canLogOut)
    #expect(helper.buttons.canCheck, "Check Again still reads")
    gate.open()
    _ = await elsewhere.value
    #expect(controller.state.helper.waiting == nil, "the line goes once the action ends")
    #expect(controller.state.helper.buttons.canSwitch)
  }

  @Test("an agent's icon comes from Settings' dependencies, never the workspace directly")
  func anAgentsIconComesFromTheDependencies() throws {
    let image = try Self.image()
    var asked: [String] = []
    let controller = SettingsController.scripted(commands: ScriptedCommands([:])) { path in
      asked.append(path)
      return path == "/Applications/Claude.app" ? image : nil
    }
    let agents = controller.state.agents.agents
    let app = try #require(agents.first { $0.id == AgentID.claude.rawValue })
    #expect(app.icon == .app(IconImage(id: "/Applications/Claude.app", image: image)))
    let codex = try #require(agents.first { $0.id == AgentID.codex.rawValue })
    #expect(codex.icon == .app(AgentLogos.codex), "an absent app uses its bundled logo")
    #expect(asked.contains("/Applications/Codex.app"), "each app is looked for through the dependencies")
  }

  /// A one-pixel image.
  ///
  /// - Returns: The image.
  /// - Throws: The failed requirement, when the image cannot be made.
  static func image() throws -> CGImage {
    let context = try #require(
      CGContext(
        data: nil,
        width: 1,
        height: 1,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
      )
    )
    return try #require(context.makeImage())
  }
}
