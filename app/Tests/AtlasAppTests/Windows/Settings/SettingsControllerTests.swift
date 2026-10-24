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

  @Test("Connect on the Claude app connects the Claude app alone")
  func connectingTheClaudeAppNamesOnlyIt() async {
    let commands = ScriptedCommands([
      "agents install": #"{"ok": true, "agents": [{"agent": "claude-app", "ok": true}]}"#,
      "agents status": #"{"ok": true, "agents": []}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.connect(AgentsStatus.claudeAppID)
    #expect(commands.ran.first == ["agents", "install", "--app"])
  }

  @Test("Check Again diagnoses the helper, says what an unready one needs, then reads the shared status again")
  func checkAgainDiagnoses() async {
    let commands = ScriptedCommands([
      "doctor": #"{"ok": true, "ready": false, "nextSteps": ["Start the helper: run atlas init (safe to run again)."]}"#
    ])
    let reads = CallCount()
    let controller = SettingsController.scripted(commands: commands, statusReads: reads)
    await controller.checkAgain()
    #expect(commands.ran == [Action.doctor.arguments])
    #expect(
      controller.state.helper.notice
        == SettingsNote(text: "Start the helper: run atlas init (safe to run again).", kind: .problem)
    )
    #expect(reads.count == 1)
  }

  @Test("while Check Again reads, VoiceOver hears each check as Checking, never the result it is replacing")
  func checkAgainSpeaksChecking() async throws {
    let commands = ScriptedCommands(["doctor": #"{"ok": true, "ready": true}"#])
    let gate = ReleaseSignal()
    commands.whileRunning { _ in await gate.wait() }
    let checks: [[String: Any]] = [["name": "helperRunning", "ok": true], ["name": "inputReady", "ok": false]]
    let controller = SettingsController.scripted(commands: commands, status: try status(["checks": checks]))
    let before = controller.state.helper.ready
    try #require(!before.isEmpty, "the Helper pane lists its checks")
    #expect(!before.contains { $0.spokenValue == "Checking" })
    controller.send(.checkAgain)
    try await settle("Check Again never started") { controller.state.helper.ready.allSatisfy(\.isChecking) }
    #expect(controller.state.helper.ready.allSatisfy { $0.spokenValue == "Checking" })
    gate.open()
    try await settle("Check Again never ended") { !controller.state.helper.ready.contains(where: \.isChecking) }
    #expect(controller.state.helper.ready.map(\.spokenValue) == before.map(\.spokenValue), "the result is heard again")
  }

  @Test("Recover in Settings says Recovered for the hold on the clock, then closes and says what it did")
  func recoverHoldsThenCloses() async throws {
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let clock = ManualClock()
    let controller = SettingsController.scripted(commands: commands, clock: clock)
    controller.send(.askRecover)
    let confirming = Task { await controller.confirmAlert() }
    let hold = Duration.seconds(Motion.Duration.doneHold.seconds)
    try await clock.untilSleeping(for: hold, "Recover never held on Recovered")
    #expect(controller.state.alert?.phase == .done(title: HelperWords.recovered))
    controller.send(.alertCancel)
    #expect(controller.session.alert == .recover, "nothing dismisses it while it holds")
    clock.advance(by: hold)
    await confirming.value
    #expect(controller.session.alert == nil)
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
    #expect(helper.waiting == HelperWords.waitForAction, "a line with a spinner says why")
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
    let app = try #require(agents.first { $0.id == AgentsStatus.claudeAppID })
    #expect(app.icon == .app(IconImage(id: "/Applications/Claude.app", image: image)))
    let codex = try #require(agents.first { $0.id == AgentID.codex.rawValue })
    #expect(codex.icon == .tile(.text(">_")), "an app the dependencies do not find keeps its tile, installed or not")
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
