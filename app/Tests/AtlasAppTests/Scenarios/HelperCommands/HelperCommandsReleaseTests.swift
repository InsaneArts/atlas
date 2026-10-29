import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// The helper's commands and the main window's releases, over one shared barrier: a release unanswered or refused
/// stays pending, holds up the next command and a reopened window, and is sent again before either goes on.
@Suite("Helper commands and control: releases")
struct HelperCommandsReleaseTests {
  @Test("a release a closed window left unanswered holds up the panel's next command and a reopened window")
  func aClosedWindowsReleaseHoldsUpThePanel() async throws {
    let service = ScriptedService()
    service.reply = HelperCommandsScenario.idle
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let actions = HelperActions(
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    )
    let viewer = try await ViewerController.watching(service, commands: commands, actions: actions)
    await viewer.control.takeControl()
    service.hold = ["release"]
    viewer.close()
    try await settle("the release was never sent") { service.held["release"] != nil }
    let panel = PanelController(
      dependencies: PanelDependencies(
        service: service,
        commands: commands,
        actions: actions,
        loginItem: ScriptedLoginItem().seam,
        clock: ManualClock()
      )
    )
    await panel.readStatus()
    let switching = Task { await panel.switchToHelper() }
    try await settle("the panel's switch never reached its wait") { actions.barrier.waiters == 1 }
    let reopened = try await ViewerController.watching(service, commands: commands, actions: actions)
    #expect(reopened.state.footer.controls.control?.isEnabled != true, "a reopened window waits for the release too")
    #expect(commands.ran.isEmpty)
    service.resume("release")
    try await settle("the panel's switch never ran") { commands.ran.count == 1 && actions.running.isEmpty }
    _ = await switching.value
    #expect(commands.ran == [Action.switchTo("atlas").arguments])
    reopened.close()
  }

  @Test("a window freed before its input and release are answered keeps Take Control waiting until both are")
  func aFreedWindowsWorkStillHoldsTheBarrier() async throws {
    let service = ScriptedService()
    service.reply = HelperCommandsScenario.idle
    let actions = HelperActions(
      commands: ScriptedCommands([:]),
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    )
    var viewer: ViewerController? = try await ViewerController.watching(service, actions: actions)
    weak let gone = viewer
    await viewer?.control.takeControl()
    service.hold = ["input"]
    viewer?.control.send(text: "a")
    try await settle("the input was never sent") { service.held["input"] != nil }
    service.hold = ["release"]
    viewer?.close()
    try await settle("the release was never sent") { service.held["release"] != nil }
    // The window and its controller are gone; what they sent is not.
    viewer = nil
    #expect(gone == nil, "the closed window's controller outlived it")
    #expect(actions.barrier.isBusy)
    let reopened = try await ViewerController.watching(service, actions: actions)
    #expect(reopened.state.footer.controls.control?.isEnabled != true, "Take Control did not wait for the gone window")
    await reopened.control.takeControl()
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "control was asked for again too soon")
    // The input answered, the release still is not: still waiting.
    service.resume("input")
    try await settle("the input was never answered") { service.held["input"] == nil }
    #expect(actions.barrier.isBusy, "the unanswered release stopped counting")
    #expect(reopened.state.footer.controls.control?.isEnabled != true)
    // Once the release is answered too, the reopened window may take control.
    service.resume("release")
    try await settle("Take Control never came back") { reopened.state.footer.controls.control?.isEnabled == true }
    await reopened.control.takeControl()
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 2)
    reopened.close()
  }

  @Test("leaving and coming back to the window while a refused release is sent again keeps nothing of that attempt")
  func aCancelledAttemptKeepsNoGrant() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    service.fail = ["release"]
    viewer.control.releaseControl()
    try await settle("the refused release never came back") {
      !viewer.barrier.isBusy && !viewer.barrier.unreleased.isEmpty
    }
    service.fail = []
    service.hold = ["release"]
    let taking = Task { await viewer.control.takeControl() }
    try await settle("the release was never sent again") { service.held["release"] != nil }
    #expect(viewer.control.acquiring, "the attempt is reserved before it waits")
    viewer.control.lostFocus()
    viewer.control.gainedFocus()
    service.resume("release")
    try await settle("the release was never answered") { viewer.barrier.unreleased.isEmpty && !viewer.barrier.isBusy }
    await taking.value
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "the cancelled attempt asked for nothing")
    #expect(viewer.control.lease == nil && !viewer.control.acquiring)
    viewer.close()
  }

  @Test("a release the service answers lease-lost is settled, so Log Out runs and does its revoke")
  func aLostLeaseIsSettled() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let commands = ScriptedCommands(["logout": #"{"ok": true}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.control.takeControl()
    // A newer viewer holds the lease now, so this window's token is obsolete.
    service.releaseRefusal = ServiceError("This token cannot release another controller", code: "lease-lost")
    viewer.control.releaseControl()
    let outcome = await viewer.actions.logOut(force: false, revoking: true)
    #expect(outcome?.ok == true)
    #expect(service.revoked)
    #expect(commands.ran == [Action.logOut(force: false).arguments])
    #expect(viewer.barrier.unreleased.isEmpty)
    viewer.close()
  }

  @Test("a release refused as uncertain stays pending, and the command goes on once its re-send is accepted")
  func anUncertainReleaseIsSentAgain() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.control.takeControl()
    service.releaseRefusal = ServiceError(
      "Control stopped accepting input, but native cleanup was not confirmed",
      code: "uncertain"
    )
    viewer.control.releaseControl()
    try await settle("the refusal never came back") { !viewer.barrier.isBusy && !viewer.barrier.unreleased.isEmpty }
    // The service keeps the unconfirmed cleanup pending for recovery on its side; a re-send it accepts settles ours.
    service.releaseRefusal = nil
    #expect(await viewer.actions.recover()?.outcome.ok == true)
    #expect(service.log.filter { $0 == "release lease" }.count == 2)
    #expect(viewer.barrier.unreleased.isEmpty)
    #expect(commands.ran == [Action.startInput.arguments])
    viewer.close()
  }
}
