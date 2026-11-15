import Foundation
import Testing

@testable import AtlasVMKit

@Suite("An earlier run's changes, recovered later")
struct DeferredRecoveryTests {
  @Test("an earlier run's window change left pending is settled once its VM is seen gone, without another restart")
  func settledLater() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    let lock = try #require(RunnerLock(at: folders.root.appending(path: "Runner.lock")))
    let off = ChangeID(String(repeating: "7c", count: 24))!
    #expect(try ledger.begin(.window(off, input: false), by: "an earlier run") == .new)
    await host.earlier(gone: false)

    await service.recover(holding: lock, vmGone: false)
    #expect(await service.answer(.outcome(off)).pending == true)

    await host.earlier(gone: true)
    let later = await service.answer(.outcome(off))
    #expect(later.outcome == .done && later.pending == false)
  }

  @Test("the framework's VM processes of this account are told whole on this Mac")
  func scanned() {
    #expect(MachineProcess.running() != nil)
  }
}
