import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// The main window's alerts and Stop and Resume beside the helper's commands: an alert's command waiting for control
/// to be given back can be called off, one asked while another runs says so, and Resume waits for a Stop unanswered.
@Suite("Helper commands and control: alerts, Stop and Resume")
struct HelperCommandsAlertTests {
  @Test("Cancel calls off an alert's command still waiting for control to be given back, and it never runs")
  func cancelCallsOffAWaitingCommand() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.control.takeControl()
    service.hold = ["release"]
    viewer.control.releaseControl()
    try await settle("the release was never sent") { service.held["release"] != nil }
    let closed = CallCount()
    let alert = AlertController(
      alert: .recover,
      actions: viewer.actions,
      links: AlertLinks(close: { _ in closed.count += 1 }, clock: ManualClock())
    )
    alert.send(.confirm)
    try await settle("Recover never reached its wait") { viewer.actions.isWaiting(.recovery) }
    alert.cancel()
    #expect(closed.count == 1, "the window closed though its command was under way")
    try await settle("Recover was never called off") { !viewer.actions.isBusy }
    service.resume("release")
    try await settle("the release was never answered") { !viewer.barrier.isBusy }
    #expect(commands.ran.isEmpty)
    viewer.close()
  }

  @Test("an alert asked while the window's own command runs says to try again, rather than vanishing")
  func anAlertAskedWhileBusySaysWhy() async throws {
    let service = ScriptedService()
    service.reply = HelperCommandsScenario.idle
    let gate = ReleaseSignal()
    let viewer = ViewerController.scripted(
      service,
      switchToHelper: {
        await gate.wait()
        return "Switched."
      }
    )
    viewer.send(.askSwitch)
    viewer.send(.alertConfirm)
    try await settle("the switch never started") { viewer.session.alertRunning }
    viewer.ask(.logOut)
    #expect(viewer.session.notice == HelperWords.waitForAction)
    gate.open()
    try await settle("the switch never ended") { !viewer.session.alertRunning }
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let held = ReleaseSignal()
    commands.whileRunning { _ in await held.wait() }
    let actions = HelperActions(
      console: SessionConsole(read: { true }),
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    )
    let clock = ManualClock()
    let window = AlertController(alert: .recover, actions: actions, links: AlertLinks(clock: clock))
    window.send(.confirm)
    try await settle("Recover never ran") { commands.ran.count == 1 }
    window.show(.logOut)
    #expect(window.alert == .recover)
    #expect(window.state.message == HelperWords.waitForAction)
    held.open()
    try await clock.untilSleeping(for: .seconds(Motion.Duration.doneHold.seconds), "Recover never held on Recovered")
    clock.advance(by: .seconds(Motion.Duration.doneHold.seconds))
    try await settle("Recover never ended") { !window.running }
  }

  @Test("Resume in the menu bar panel waits for a Stop the main window sent and the service has not answered")
  func resumeWaitsAcrossWindows() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "stopped", "state": "stopped"]]
    let commands = ScriptedCommands([:])
    let viewer = try await ViewerController.watching(service, commands: commands)
    let panel = PanelController(
      dependencies: PanelDependencies(
        service: service,
        commands: commands,
        actions: viewer.actions,
        loginItem: ScriptedLoginItem().seam,
        clock: ManualClock()
      )
    )
    service.hold = ["hold"]
    let stopping = Task { await viewer.setStopped(true) }
    try await settle("the Stop was never sent") { service.held["hold"] != nil }
    await panel.resumeAgent()
    #expect(service.log == ["stop"], "the panel's Resume waited")
    service.resume("hold")
    try await settle("the Stop was never answered") { viewer.actions.agentRequests == 0 }
    await stopping.value
    await panel.resumeAgent()
    #expect(service.log == ["stop", "resume"])
    viewer.close()
  }
}
