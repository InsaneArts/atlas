import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// The helper's commands and the main window's acquisitions, over one shared barrier: no command runs while an
/// acquisition is unanswered or a lost one may still hold a lease, a late grant is given back first, and Take Control
/// waits for the person's own input still in flight.
@Suite("Helper commands and control: grants")
struct HelperCommandsGrantTests {
  @Test("a grant that arrives after a tab change and a helper command is given back before the command runs")
  func aLateGrantIsGivenBackBeforeACommand() async throws {
    let service = ScriptedService()
    service.reply = HelperCommandsScenario.idle
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let seen = CommandLog()
    commands.whileRunning { _ in seen.serviceLogs.append(service.log) }
    let viewer = try await ViewerController.watching(service, commands: commands)
    service.hold = ["acquire"]
    let taking = Task { await viewer.control.takeControl() }
    try await settle("control was never asked for") { service.held["acquire"] != nil }
    viewer.send(.show(.apps))
    let switching = Task { await viewer.actions.switchTo("atlas") }
    try await settle("the switch never reached its wait") { viewer.barrier.waiters == 1 }
    #expect(commands.ran.isEmpty, "nothing runs while the acquisition is unanswered")
    viewer.send(.show(.screen))
    #expect(viewer.state.footer.controls.control?.isEnabled != true)
    service.resume("acquire")
    try await settle("the switch never ran") { commands.ran.count == 1 && !viewer.actions.isBusy }
    #expect(await switching.value?.ok == true)
    await taking.value
    #expect(viewer.control.lease == nil)
    #expect(seen.serviceLogs.first?.contains("released lease") == true, "the late grant was given back first")
    viewer.close()
  }

  @Test("Take Control waits while a helper command runs, and a grant that arrives as one starts is given back")
  func takeControlWaitsForACommand() async throws {
    let service = ScriptedService()
    service.reply = HelperCommandsScenario.idle
    let commands = ScriptedCommands(["switch": #"{"ok": true}"#])
    let gate = ReleaseSignal()
    let seen = CommandLog()
    commands.whileRunning { _ in
      seen.serviceLogs.append(service.log)
      await gate.wait()
    }
    let viewer = try await ViewerController.watching(service, commands: commands)
    service.hold = ["acquire"]
    let taking = Task { await viewer.control.takeControl() }
    try await settle("control was never asked for") { service.held["acquire"] != nil }
    let switching = Task { await viewer.actions.switchTo("atlas") }
    try await settle("the switch never reached its wait") { viewer.barrier.waiters == 1 }
    service.resume("acquire")
    try await settle("the switch never ran") { seen.serviceLogs.count == 1 }
    await taking.value
    #expect(viewer.control.lease == nil, "a grant that arrives once a command started is not kept")
    #expect(seen.serviceLogs.first?.contains("released lease") == true)
    #expect(
      viewer.state.footer.controls.control?.isEnabled != true,
      "the command still runs after its release was answered"
    )
    await viewer.control.takeControl()
    #expect(service.log.filter { $0.hasPrefix("acquire") }.count == 1, "nothing is asked while it runs")
    gate.open()
    try await settle("Take Control never came back") { viewer.state.footer.controls.control?.isEnabled == true }
    #expect(await switching.value?.ok == true)
    viewer.close()
  }

  @Test(
    "a lost acquisition reply holds the next helper command until a lease it granted can no longer be alive and a status answers, whoever holds control then"
  )
  func aLostAcquisitionHoldsCommands() async throws {
    let service = ScriptedService()
    service.reply = HelperCommandsScenario.idle
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
    await viewer.control.takeControl()
    #expect(viewer.control.lease == nil)
    try #require(service.leaseHeld == true, "the service granted control though its reply was lost")
    let switching = Task { await actions.switchTo("atlas") }
    try await clock.untilSleeping(for: lifetime, "the hold never waited out a lease's lifetime")
    try await settle("the switch never reached its wait") { actions.barrier.waiters == 1 }
    #expect(commands.ran.isEmpty, "nothing runs while a lease it may have granted is alive")
    #expect(viewer.state.footer.controls.control?.isEnabled != true)
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

  @Test("a lost acquisition no status confirms over refuses the next command, saying why, until the service answers")
  func anUnconfirmedLostAcquisitionRefuses() async throws {
    let service = ScriptedService()
    service.reply = HelperCommandsScenario.idle
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
    await viewer.control.takeControl()
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

  @Test("the person's own input still in flight after Give Back holds Take Control until it is answered")
  func ownInputHoldsTakeControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    service.hold = ["input"]
    viewer.control.sendKey("escape")
    try await settle("the key was never sent") { service.held["input"] != nil }
    viewer.control.releaseControl()
    try await settle("the release was never answered") { service.log.contains("released lease") }
    #expect(viewer.state.footer.controls.control?.isEnabled != true, "the batch in flight holds the helper")
    service.resume("input")
    try await settle("Take Control never came back") { viewer.state.footer.controls.control?.isEnabled == true }
    viewer.close()
  }
}
