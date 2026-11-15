import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A runner giving way to a newer build")
struct RetireTests {
  @Test("it gives way only once its VM is seen stopped, and from then takes no change")
  func onlyOnceStopped() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))

    let running = await service.retire()
    _ = await service.answer(.stop(.repeating("e0")))
    await host.blinding()
    let untold = await service.retire()
    await host.seeing()
    let stopped = await service.retire()
    let started = await service.answer(.start(.repeating("e1")))

    #expect(!running && !untold && stopped)
    #expect(started.ok == false && started.outcome == nil)
  }

  @Test("it never gives way while a change is being done or the VM is being removed")
  func neverMidChange() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    await host.holding()
    let starting = Task { await service.answer(.start(.repeating("e2"))) }
    await host.untilHeld()

    let midChange = await service.retire()
    await host.release()
    _ = await starting.value
    _ = await service.answer(.forget(.repeating("e3")))
    let forgetting = await service.retire()

    #expect(!midChange && !forgetting)
  }

  @Test(
    "it never gives way with its window open, and once retiring a pending change asked again is refused, never resumed"
  )
  func neverWithAWindowOrAResume() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    let request = InstallRequest(bundle: "/Applications/Atlas.app", enrolment: "e", secret: "s")
    _ = await service.answer(.install(.repeating("e4"), request))
    _ = await service.answer(.window(.repeating("e5"), input: true))

    let windowed = await service.retire()
    _ = await service.answer(.window(.repeating("e6"), input: false))
    let closed = await service.retire()
    let again = await service.answer(.install(.repeating("e4"), request))

    #expect(!windowed && closed)
    #expect(again.ok == false, "refused, for the next run to answer")
    #expect(await host.installs.count == 1)
  }

  @Test("no report or removal comes in while it looks whether it may give way")
  func closedWhileItLooks() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    await host.holdingPower()
    let retiring = Task { await service.retire() }
    await host.untilPowerHeld()

    let report = await service.answer(.closed(.repeating("e7"), install: .repeating("e8")))
    let forgotten = await service.answer(.forget(.repeating("e9")))
    await host.releasePower()
    let retired = await retiring.value

    #expect(report.ok == false && forgotten.ok == false)
    #expect(retired)
    #expect(await host.reports.isEmpty)
    #expect(await host.forgotten == 0)
  }
}
