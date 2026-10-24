import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// Recover and the main window's control, over one shared barrier: it ends this window's control, waits for every
/// release, sends a refused one again first, and is reserved before it reads, so Take Control waits.
@Suite("Helper commands and control: Recover")
struct HelperCommandsRecoverTests {
  @Test("Recover waits for a release still unanswered")
  func recoverWaitsForAHeldRelease() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let seen = CommandLog()
    commands.whileRunning { _ in seen.serviceLogs.append(service.log) }
    let viewer = try await ViewerController.watching(service, commands: commands)
    await viewer.control.takeControl()
    service.hold = ["release"]
    service.reply = ["ready": false, "canObserve": true, "overview": ["mode": "blocked", "recoverable": true]]
    await viewer.readingLoop.readStatus()
    try #require(viewer.control.lease == nil, "lost readiness ended control")
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
    await viewer.control.takeControl()
    service.fail = ["release"]
    viewer.control.releaseControl()
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

  @Test("Recover ends this window's control and is reserved before it reads the status, so Take Control waits")
  func recoverEndsControlAndReservesFirst() async throws {
    let service = ScriptedService()
    let commands = ScriptedCommands(["start-input": #"{"ok": true}"#])
    let seen = CommandLog()
    commands.whileRunning { _ in seen.serviceLogs.append(service.log) }
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, commands: commands, clock: clock)
    await viewer.control.takeControl()
    try await clock.untilSleeping(2, for: ViewerReadingLoop.controlEvery, "the viewer never came to rest in control")
    // Recover's own reading, made once it no longer waits for the release: the viewer reads the status too once its
    // release is answered, and which of the two asks first is the executor's choice.
    service.holdWhen = { route in
      route == "status" && viewer.actions.running.contains(.recovery) && !viewer.actions.isWaiting(.recovery)
    }
    let recovering = Task { await viewer.actions.recover() }
    try await settle("Recover never read the status") { service.held["status"] != nil }
    #expect(viewer.control.lease == nil, "control was given back first")
    #expect(!viewer.state.top.actions.control.isEnabled, "Take Control waits while it reads")
    service.resume("status")
    try await settle("Recover never ran") { commands.ran.count == 1 && !viewer.actions.isBusy }
    #expect(await recovering.value?.outcome.ok == true)
    #expect(seen.serviceLogs.first?.contains("released lease") == true)
    viewer.close()
  }
}
