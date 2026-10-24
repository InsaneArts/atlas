import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("PanelController")
struct PanelControllerTests {
  @Test("the panel shows the service's status, and no picture when capture fails")
  func showsTheServiceStatus() async {
    let service = ScriptedService()
    service.reply = ["overview": ["mode": "recovering", "account": "atlas"]]
    service.fail = ["frame"]
    let controller = PanelController.scripted(service)
    await controller.readStatus()
    await controller.readFrame()
    #expect(controller.status.overview.mode == .recovering)
    #expect(controller.picture == nil)
  }

  @Test("only the service's answer is handed to the viewer: none before the first reading, none after a failed one")
  func onlyAnAnswerIsHandedOn() async {
    let service = ScriptedService()
    // Every field at its default, as a failed reading also shows: an answer all the same.
    service.reply = [:]
    let controller = PanelController.scripted(service)
    #expect(controller.answer == nil && controller.answered == nil)
    await controller.readStatus()
    #expect(controller.answer == ViewerStatus())
    service.fail = ["status"]
    await controller.readStatus()
    #expect(controller.answer == nil && controller.answered == false)
  }

  @Test("a reading that fails after answers reads as Reconnecting…, and the next answer ends it")
  func aFailedReadingReconnects() async {
    let service = ScriptedService()
    service.reply = ["overview": ["mode": "observing", "state": "idle"]]
    let controller = PanelController.scripted(service)
    #expect(controller.mode.kind == .connecting)
    #expect(controller.state.status.line == "Starting the helper…")
    await controller.readStatus()
    #expect(controller.mode.kind == .idle)
    service.fail = ["status"]
    await controller.readStatus()
    #expect(controller.mode.kind == .connecting)
    #expect(controller.state.status.line == "Reconnecting…")
    service.fail = []
    await controller.readStatus()
    #expect(controller.mode.kind == .idle)
  }

  @Test("each reading is handed to the rest of the app, the service's answer as itself and a failed one as nil")
  func readingsAreHandedOn() async {
    let service = ScriptedService()
    service.reply = ["overview": ["mode": "recovering", "state": "needsYou"]]
    var read: [ViewerStatus?] = []
    let controller = PanelController.scripted(service, links: PanelLinks(read: { read.append($0) }))
    await controller.readStatus()
    service.fail = ["status"]
    await controller.readStatus()
    #expect(read.map { $0?.overview.state } == [.needsYou, nil])
  }

  @Test("the picture opens the main window on Screen, All activity on Activity and Open Atlas on the tab it shows")
  func theOpenersOpenTheirTabs() {
    var opened: [ViewerTab?] = []
    let controller = PanelController.scripted(links: PanelLinks(openViewer: { opened.append($0) }))
    controller.send(.openScreen)
    controller.send(.openActivity)
    controller.send(.openAtlas)
    #expect(opened == [.screen, .activity, nil])
  }

  @Test("Recover… and Switch to the Helper… ask first and run nothing themselves")
  func theHelperCommandsAskFirst() {
    var asked: [HelperAlert] = []
    let commands = ScriptedCommands([:])
    let controller = PanelController.scripted(commands: commands, links: PanelLinks(showAlert: { asked.append($0) }))
    controller.send(.askRecover)
    controller.send(.askSwitch)
    #expect(asked == [.recover, .switchToHelper])
    #expect(commands.ran.isEmpty)
  }

  @Test("Open Settings opens the Settings window")
  func openSettingsOpensSettings() {
    var opened = 0
    let controller = PanelController.scripted(links: PanelLinks(openSettings: { opened += 1 }))
    controller.send(.openSettings)
    #expect(opened == 1)
  }

  @Test("stopping the agent asks the service, and resuming lets it go on")
  func stoppingTheAgent() async {
    let service = ScriptedService()
    let controller = PanelController.scripted(service)
    await controller.stopAgent()
    await controller.resumeAgent()
    #expect(service.log == ["stop", "resume"])
    #expect(controller.session.requests == 0)
  }

  @Test("a stop the service refuses says why in place of the line, until the panel opens again")
  func refusedStop() async {
    let service = ScriptedService()
    service.fail = ["hold"]
    let controller = PanelController.scripted(service)
    await controller.stopAgent()
    #expect(controller.state.notice == "hold failed.")
    controller.watch(open: true)
    #expect(controller.state.notice == nil)
    controller.watch(open: false)
  }

  @Test("a stop never waits for the panel's own requests, and a resume waits until every stop is answered")
  func stopNeverWaits() async throws {
    let service = ScriptedService()
    service.hold = ["hold"]
    let controller = PanelController.scripted(service)
    let first = Task { await controller.stopAgent() }
    try await settle("the first stop never reached the service") { service.held["hold"] != nil }
    await controller.resumeAgent()
    #expect(service.log == ["stop"], "a resume waits")
    await controller.stopAgent()
    #expect(service.log == ["stop", "stop"], "a second stop does not, and is answered")
    await controller.resumeAgent()
    #expect(service.log == ["stop", "stop"], "the first stop is still unanswered, so a resume still waits")
    service.resume("hold")
    await first.value
    await controller.resumeAgent()
    #expect(service.log == ["stop", "stop", "resume"])
  }

  @Test("the open panel reads the status and the picture every 2 s, and the closed panel the status every 5 s")
  func theReadingsKeepTheirIntervals() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let controller = PanelController.scripted(service, clock: clock)
    controller.watch(open: true)
    try await clock.untilSleeping(2, for: PanelController.openEvery, "the open panel never paused after reading")
    #expect(service.statusCalls == 1 && service.frameCalls == 1)
    #expect(clock.waiting == [PanelController.openEvery, PanelController.openEvery])
    clock.advance(by: PanelController.openEvery)
    try await clock.untilSleeping(2, for: PanelController.openEvery, "the open panel never read again")
    #expect(service.statusCalls == 2 && service.frameCalls == 2)
    controller.watch(open: false)
    try await clock.untilSleeping(for: PanelController.closedEvery, "the closed panel never paused after reading")
    #expect(clock.waiting == [PanelController.closedEvery], "a closed panel reads no pictures")
    let pictures = service.frameCalls
    clock.advance(by: PanelController.closedEvery)
    try await settle("the closed panel never read the status again") { service.statusCalls == 4 }
    #expect(service.frameCalls == pictures, "a closed panel reads no pictures")
  }

  @Test(
    "pausing or closing with a capture outstanding drops its late reply and reads no more pictures, and reopening reads at once",
    arguments: [true, false]
  )
  func aLateCaptureIsDropped(pausing: Bool) async throws {
    let service = ScriptedService()
    service.picture = png()
    let clock = ManualClock()
    let controller = PanelController.scripted(service, clock: clock)
    controller.watch(open: true)
    try await clock.untilSleeping(2, for: PanelController.openEvery, "the open panel never paused after reading")
    try await settle("the first picture never showed") { controller.picture != nil }
    let first = controller.picture?.id
    service.hold = ["frame"]
    clock.advance(by: PanelController.openEvery)
    try await settle("the next capture was never asked for") { service.held["frame"] != nil }
    if pausing { controller.send(.togglePause) } else { controller.watch(open: false) }
    service.resume("frame")
    // The status reading's next pause, open or closed, then the reading after it.
    let next = pausing ? PanelController.openEvery : PanelController.closedEvery
    try await clock.untilSleeping(for: next, "the status was never read again")
    clock.advance(by: next)
    try await settle("the status was never read again") { service.statusCalls >= 3 }
    #expect(controller.picture?.id == first, "the late reply shows nothing")
    #expect(service.frameCalls == 2, "no picture is read meanwhile")
    if pausing { controller.send(.togglePause) } else { controller.watch(open: true) }
    try await settle("reopening never read a picture") { service.frameCalls == 3 }
    controller.watch(open: false)
  }

  @Test(
    "pausing the picture only stops its pictures: the service and the command line hear nothing, so the agent goes on"
  )
  func pausingTouchesOnlyThePicture() async throws {
    let service = ScriptedService()
    service.picture = png()
    let commands = ScriptedCommands([:])
    let clock = ManualClock()
    let controller = PanelController.scripted(service, commands: commands, clock: clock)
    controller.watch(open: true)
    try await clock.untilSleeping(2, for: PanelController.openEvery, "the open panel never paused after reading")
    try await settle("opening never listed the helper") { !commands.ran.isEmpty }
    let ran = commands.ran
    controller.send(.togglePause)
    #expect(controller.state.picture.pause.isPaused)
    clock.advance(by: PanelController.openEvery)
    try await settle("the status was never read again") { service.statusCalls >= 2 }
    controller.send(.togglePause)
    #expect(!controller.state.picture.pause.isPaused)
    #expect(service.log.isEmpty, "no stop, resume, release or recovery was asked of the service")
    #expect(Set(commands.ran) == Set(ran), "no command ran but the helper's listing, which each opening reads again")
    controller.watch(open: false)
  }

  @Test("switching locks the screen for the helper the service reports and says what to do next")
  func switchingLocksTheScreen() async {
    let service = ScriptedService()
    service.reply = ["overview": ["mode": "loggedOut", "account": "atlas", "accountName": "Atlas"]]
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let controller = PanelController.scripted(service, commands: commands)
    await controller.readStatus()
    #expect(await controller.logIn() == PanelWords.logInThenSwitchBack(to: "Atlas"))
    #expect(commands.ran == [Action.switchTo("atlas").arguments])
  }

  @Test("a helper not found at launch is listed again when the panel opens, and a later failed listing keeps it known")
  func theHelperIsListedAgainOnOpening() async throws {
    let commands = ScriptedCommands([:])
    let controller = PanelController.scripted(commands: commands)
    await controller.findHelper()
    #expect(controller.helper == nil)
    commands.answer("switch --list", #"{"ok": true, "accounts": [{"name": "atlas", "helper": true}]}"#)
    controller.watch(open: true)
    try await settle("the helper was never listed again") { controller.helper == "atlas" }
    // A listing that fails afterwards keeps the helper known.
    commands.answer("switch --list", #"{"ok": false, "error": "No"}"#)
    await controller.findHelper()
    #expect(controller.helper == "atlas")
    controller.watch(open: false)
  }

  @Test("switching with no helper account found says where to look, and runs nothing")
  func switchWithNoHelperSaysWhereToLook() async {
    let commands = ScriptedCommands([:])
    let controller = PanelController.scripted(commands: commands)
    #expect(await controller.logIn() == PanelWords.helperNotFound)
    #expect(commands.ran == [Action.listAccounts.arguments], "it only looked for the helper")
  }

  @Test("a switch that fails tells the window that asked why")
  func aFailedSwitchSaysWhy() async {
    let service = ScriptedService()
    service.reply = ["overview": ["mode": "loggedOut", "account": "atlas"]]
    let commands = ScriptedCommands(["switch": #"{"ok": false, "error": "The screen could not lock"}"#])
    let controller = PanelController.scripted(service, commands: commands)
    await controller.readStatus()
    #expect(await controller.logIn() == "The screen could not lock")
  }

  @Test("Open at Login goes through atlas, which records the choice, then follows the system; a refusal says why")
  func openAtLoginFollowsTheSystem() async {
    let loginItem = ScriptedLoginItem(available: true)
    let commands = ScriptedCommands(["menubar --login": #"{"ok": true}"#])
    // The command turns the login item on or off, as the app's own --login does.
    commands.whileRunning { arguments in loginItem.enabled = arguments.last == "on" }
    let controller = PanelController.scripted(commands: commands, loginItem: loginItem)
    await controller.setOpenAtLogin(true)
    #expect(commands.ran == [Action.openAtLogin(true).arguments])
    #expect(controller.opensAtLogin == OpenAtLogin.State(isAvailable: true, isEnabled: true))
    commands.whileRunning { _ in }
    commands.answer("menubar --login", #"{"ok": false, "error": "Not allowed"}"#)
    await controller.setOpenAtLogin(false)
    #expect(controller.opensAtLogin.isEnabled, "the system kept it on")
    #expect(controller.state.notice == "Not allowed.")
  }

  @Test(
    "quick Open at Login clicks run one change at a time, then only the newest that differs, and the tick holds the click until the last ends"
  )
  func openAtLoginChangesRunOneAtATime() async {
    let loginItem = ScriptedLoginItem(available: true)
    let commands = ScriptedCommands(["menubar --login": #"{"ok": true}"#])
    let controller = PanelController.scripted(commands: commands, loginItem: loginItem)
    let seen = LoginChangeObservation()
    commands.whileRunning { arguments in
      loginItem.enabled = arguments.last == "on"
      guard commands.ran.count == 1 else { return }
      await controller.setOpenAtLogin(false)
      await controller.setOpenAtLogin(true)
      await controller.setOpenAtLogin(false)
      // Opening the panel meanwhile reads the login item, which the first change has just turned on.
      controller.readOpenAtLogin()
      seen.startedAnother = commands.ran.count > 1
      seen.switchOn = controller.opensAtLogin.isEnabled
    }
    await controller.setOpenAtLogin(true)
    #expect(seen.startedAnother == false, "no second change started while the first ran")
    #expect(seen.switchOn == false, "the tick kept the newest click")
    #expect(commands.ran == [Action.openAtLogin(true).arguments, Action.openAtLogin(false).arguments])
    #expect(controller.opensAtLogin == OpenAtLogin.State(isAvailable: true, isEnabled: false))
  }

  @Test("a status the service refuses to give, as a busy one does, leaves the last one standing")
  func aRefusedStatusLeavesTheLastOne() async {
    let service = ScriptedService()
    let controller = PanelController.scripted(service)
    await controller.readStatus()
    service.busy = true
    await controller.readStatus()
    #expect(controller.status.overview.mode == .observing)
    #expect(controller.answer != nil)
  }

  @Test("each opening reads Open at Login again, so a change made elsewhere meanwhile shows in Settings")
  func openAtLoginIsReadAgainOnOpening() {
    let loginItem = ScriptedLoginItem(available: true, enabled: true)
    let controller = PanelController.scripted(loginItem: loginItem)
    controller.watch(open: true)
    #expect(controller.opensAtLogin.isEnabled)
    controller.watch(open: false)
    // Getting Started turns it off while the panel is closed.
    loginItem.enabled = false
    controller.watch(open: true)
    #expect(!controller.opensAtLogin.isEnabled)
    controller.watch(open: false)
  }
}
