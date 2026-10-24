import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// The helper's commands and the main window's control, over one shared barrier: no command runs while an
/// acquisition or a release is unanswered, a late grant is given back first, and Take Control waits for both.
@MainActor
@Suite("Helper commands and control")
struct HelperCommandsControlTests {
  /// A status of a ready, idle helper the panel can switch to.
  static let idle: [String: Any] = [
    "ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle", "account": "atlas"],
  ]

  @Test("a grant that arrives after a tab change and a helper command is given back before the command runs")
  func aLateGrantIsGivenBackBeforeACommand() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let seen = CommandLog()
    commands.whileRunning { _ in seen.serviceLogs.append(service.log) }
    let viewer = try await ViewerController.watching(service, commands: commands)
    service.hold = ["acquire"]
    let taking = Task { await viewer.takeControl() }
    try await settle("control was never asked for") { service.held["acquire"] != nil }
    viewer.send(.selectTab(.apps))
    let switching = Task { await viewer.actions.switchTo("atlas") }
    try await settle("the switch never reached its wait") { viewer.barrier.waiters == 1 }
    #expect(commands.ran.isEmpty, "nothing runs while the acquisition is unanswered")
    viewer.send(.selectTab(.screen))
    #expect(!viewer.state.top.actions.control.isEnabled)
    service.resume("acquire")
    try await settle("the switch never ran") { commands.ran.count == 1 && !viewer.actions.isBusy }
    #expect(await switching.value?.ok == true)
    await taking.value
    #expect(viewer.session.lease == nil)
    #expect(seen.serviceLogs.first?.contains("released lease") == true, "the late grant was given back first")
    viewer.close()
  }

  @Test("Take Control waits while a helper command runs, and a grant that arrives as one starts is given back")
  func takeControlWaitsForACommand() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let gate = ReleaseSignal()
    let seen = CommandLog()
    commands.whileRunning { _ in
      seen.serviceLogs.append(service.log)
      await gate.wait()
    }
    let viewer = try await ViewerController.watching(service, commands: commands)
    service.hold = ["acquire"]
    let taking = Task { await viewer.takeControl() }
    try await settle("control was never asked for") { service.held["acquire"] != nil }
    let switching = Task { await viewer.actions.switchTo("atlas") }
    try await settle("the switch never reached its wait") { viewer.barrier.waiters == 1 }
    service.resume("acquire")
    try await settle("the switch never ran") { commands.ran.count == 1 }
    await taking.value
    #expect(viewer.session.lease == nil, "a grant that arrives once a command started is not kept")
    #expect(seen.serviceLogs.first?.contains("released lease") == true)
    #expect(!viewer.state.top.actions.control.isEnabled, "the command still runs after its release was answered")
    await viewer.takeControl()
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "nothing is asked while it runs")
    gate.open()
    try await settle("Take Control never came back") { viewer.state.top.actions.control.isEnabled }
    #expect(await switching.value?.ok == true)
    viewer.close()
  }

  @Test("Recover waits for a release still unanswered")
  func recoverWaitsForAHeldRelease() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let seen = CommandLog()
    commands.whileRunning { _ in seen.serviceLogs.append(service.log) }
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.takeControl()
    service.hold = ["release"]
    service.reply = ["ready": false, "canObserve": true, "overview": ["mode": "blocked", "recoverable": true]]
    await viewer.readStatus()
    try #require(viewer.session.lease == nil, "lost readiness ended control")
    try await settle("the release was never sent") { service.held["release"] != nil }
    let recovering = Task { await viewer.actions.recover()?.outcome }
    try await settle("Recover never reached its wait") { viewer.barrier.waiters == 1 }
    #expect(commands.ran.isEmpty)
    service.resume("release")
    try await settle("Recover never ran") { commands.ran.count == 1 && !viewer.actions.isBusy }
    #expect(await recovering.value?.ok == true)
    #expect(seen.serviceLogs.first?.contains("released lease") == true, "the release was answered before Recover ran")
    viewer.close()
  }

  @Test("a refused release stays pending: Recover sends it again first, and stops while it is still refused")
  func aRefusedReleaseStopsRecover() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.takeControl()
    service.fail = ["release"]
    viewer.releaseControl()
    let refused = await viewer.actions.recover()?.outcome
    #expect(refused?.ok == false)
    #expect(refused?.message == "release failed")
    #expect(commands.ran.isEmpty)
    #expect(service.log.filter { $0 == "release lease" }.count == 2, "sent, then sent again before Recover")
    service.fail = []
    #expect(await viewer.actions.recover()?.outcome.ok == true)
    #expect(service.log.filter { $0 == "release lease" }.count == 3)
    viewer.close()
  }

  @Test("a release a closed window left unanswered holds up the panel's next command and a reopened window")
  func aClosedWindowsReleaseHoldsUpThePanel() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let actions = HelperActions(
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    )
    let viewer = try await ViewerController.watching(service, commands: commands, actions: actions)
    await viewer.takeControl()
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
    #expect(!reopened.state.top.actions.control.isEnabled, "a reopened window waits for the release too")
    #expect(commands.ran.isEmpty)
    service.resume("release")
    try await settle("the panel's switch never ran") { commands.ran.count == 1 && actions.running.isEmpty }
    _ = await switching.value
    #expect(commands.ran == [Action.switchTo("atlas").arguments])
    reopened.close()
  }

  @Test(
    "a lost acquisition reply holds the next helper command until a lease it granted can no longer be alive and a status answers, whoever holds control then"
  )
  func aLostAcquisitionHoldsCommands() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let lifetime = ControlBarrier.lateGrant + ControlBarrier.leaseLifetime
    let clock = ManualClock()
    let actions = HelperActions(
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: clock)
    )
    let viewer = try await ViewerController.watching(
      service,
      commands: commands,
      actions: actions
    )
    service.lostAcquireReplies = 1
    await viewer.takeControl()
    #expect(viewer.session.lease == nil)
    try #require(service.leaseHeld == true, "the service granted control though its reply was lost")
    let switching = Task { await actions.switchTo("atlas") }
    try await clock.untilSleeping(for: lifetime, "the hold never waited out a lease's lifetime")
    try await settle("the switch never reached its wait") { actions.barrier.waiters == 1 }
    #expect(commands.ran.isEmpty, "nothing runs while a lease it may have granted is alive")
    #expect(!viewer.state.top.actions.control.isEnabled)
    #expect(!service.revoked, "no viewer's control is revoked for it")
    clock.advance(by: lifetime - .milliseconds(1))
    #expect(clock.waiting == [.milliseconds(1)], "the hold lasts the whole lifetime")
    #expect(commands.ran.isEmpty)
    // The lifetime passed; another viewer, such as an agent pane, may hold control now, which prolongs nothing.
    clock.advance(by: .milliseconds(1))
    try await settle("the switch never ran") { commands.ran.count == 1 && !actions.isBusy }
    #expect(await switching.value?.ok == true)
    viewer.close()
  }

  @Test("leaving and coming back to the window while a refused release is sent again keeps nothing of that attempt")
  func aCancelledAttemptKeepsNoGrant() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    service.fail = ["release"]
    viewer.releaseControl()
    try await settle("the refused release never came back") {
      !viewer.barrier.isBusy && !viewer.barrier.unreleased.isEmpty
    }
    service.fail = []
    service.hold = ["release"]
    let taking = Task { await viewer.takeControl() }
    try await settle("the release was never sent again") { service.held["release"] != nil }
    #expect(viewer.session.acquiring, "the attempt is reserved before it waits")
    viewer.lostFocus()
    viewer.gainedFocus()
    service.resume("release")
    try await settle("the release was never answered") { viewer.barrier.unreleased.isEmpty && !viewer.barrier.isBusy }
    await taking.value
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "the cancelled attempt asked for nothing")
    #expect(viewer.session.lease == nil && !viewer.session.acquiring)
    viewer.close()
  }

  @Test("a release the service answers lease-lost is settled, so Log Out runs and does its revoke")
  func aLostLeaseIsSettled() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    let commands = ScriptedCommands(["logout": #"{"ok": true}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.takeControl()
    // A newer viewer holds the lease now, so this window's token is obsolete.
    service.releaseRefusal = ServiceError("This token cannot release another controller", code: "lease-lost")
    viewer.releaseControl()
    let outcome = await viewer.actions.logOut(force: false, revoking: true)
    #expect(outcome?.ok == true)
    #expect(service.revoked)
    #expect(commands.ran == [Action.logOut(force: false).arguments])
    #expect(viewer.barrier.unreleased.isEmpty)
    viewer.close()
  }

  @Test("a lost acquisition no status confirms over refuses the next command, saying why, until the service answers")
  func anUnconfirmedLostAcquisitionRefuses() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let lifetime = ControlBarrier.lateGrant + ControlBarrier.leaseLifetime
    let clock = ManualClock()
    let actions = HelperActions(
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: clock)
    )
    let viewer = try await ViewerController.watching(
      service,
      commands: commands,
      actions: actions
    )
    service.lostAcquireReplies = 1
    await viewer.takeControl()
    let switching = Task { await actions.switchTo("atlas") }
    try await clock.untilSleeping(for: lifetime, "the hold never waited out a lease's lifetime")
    try await settle("the switch never reached its wait") { actions.barrier.waiters == 1 }
    let asked = service.statusCalls
    service.fail = ["status"]
    clock.advance(by: lifetime)
    // Each failed reading waits its second on the clock before the next.
    for attempt in 1..<ControlBarrier.statusTries {
      try await clock.untilSleeping(for: ControlBarrier.statusEvery, "reading \(attempt) never waited its second")
      clock.advance(by: ControlBarrier.statusEvery)
    }
    try await settle("the switch never ended") { !actions.isBusy }
    #expect(await switching.value?.message == HelperWords.helperUnconfirmed)
    #expect(service.statusCalls - asked == ControlBarrier.statusTries + 1, "it gave up after its tries, then once more")
    #expect(commands.ran.isEmpty)
    service.fail = []
    #expect(await actions.switchTo("atlas")?.ok == true, "once the service answers, commands run")
    viewer.close()
  }

  @Test("Cancel calls off an alert's command still waiting for control to be given back, and it never runs")
  func cancelCallsOffAWaitingCommand() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.takeControl()
    service.hold = ["release"]
    viewer.releaseControl()
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

  @Test("Recover ends this window's control and is reserved before it reads the status, so Take Control waits")
  func recoverEndsControlAndReservesFirst() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let seen = CommandLog()
    commands.whileRunning { _ in seen.serviceLogs.append(service.log) }
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, commands: commands, clock: clock)
    await viewer.takeControl()
    // The reading the grant asks for has ended once the heartbeat and the pause between pictures both wait, so the
    // status held next is Recover's own.
    try await clock.untilSleeping(2, for: ViewerController.controlEvery, "the viewer never came to rest in control")
    service.hold = ["status"]
    let recovering = Task { await viewer.actions.recover() }
    try await settle("Recover never read the status") { service.held["status"] != nil }
    #expect(viewer.session.lease == nil, "control was given back first")
    #expect(!viewer.state.top.actions.control.isEnabled, "Take Control waits while it reads")
    service.resume("status")
    try await settle("Recover never ran") { commands.ran.count == 1 && !viewer.actions.isBusy }
    #expect(await recovering.value?.outcome.ok == true)
    #expect(seen.serviceLogs.first?.contains("released lease") == true)
    viewer.close()
  }

  @Test("an alert asked while the window's own command runs says to try again, rather than vanishing")
  func anAlertAskedWhileBusySaysWhy() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
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
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    )
    let window = AlertController(alert: .recover, actions: actions, links: AlertLinks(clock: ManualClock()))
    window.send(.confirm)
    try await settle("Recover never ran") { commands.ran.count == 1 }
    window.show(.logOut)
    #expect(window.alert == .recover)
    #expect(window.state.message == HelperWords.waitForAction)
    held.open()
    try await settle("Recover never ended") { !window.running }
  }

  @Test("the person's own input still in flight after Give Back holds Take Control until it is answered")
  func ownInputHoldsTakeControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    service.hold = ["input"]
    viewer.sendKey("escape")
    try await settle("the key was never sent") { service.held["input"] != nil }
    viewer.releaseControl()
    try await settle("the release was never answered") { service.log.contains("released lease") }
    #expect(!viewer.state.top.actions.control.isEnabled, "the batch in flight holds the helper")
    service.resume("input")
    try await settle("Take Control never came back") { viewer.state.top.actions.control.isEnabled }
    viewer.close()
  }

  @Test("a release refused as uncertain stays pending, and the command goes on once its re-send is accepted")
  func anUncertainReleaseIsSentAgain() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.takeControl()
    service.releaseRefusal = ServiceError(
      "Control stopped accepting input, but native cleanup was not confirmed",
      code: "uncertain"
    )
    viewer.releaseControl()
    try await settle("the refusal never came back") { !viewer.barrier.isBusy && !viewer.barrier.unreleased.isEmpty }
    // The service keeps the unconfirmed cleanup pending for recovery on its side; a re-send it accepts settles ours.
    service.releaseRefusal = nil
    #expect(await viewer.actions.recover()?.outcome.ok == true)
    #expect(service.log.filter { $0 == "release lease" }.count == 2)
    #expect(viewer.barrier.unreleased.isEmpty)
    #expect(commands.ran == [Action.startInput.arguments])
    viewer.close()
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
