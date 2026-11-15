import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The control service's own pending changes")
struct PendingTests {
  let change = ChangeID.repeating("9e")

  @Test(
    "a change left pending because its effect could not be seen is settled once it can be, without being done again"
  )
  func seenLater() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    await host.blinding()

    let first = await service.answer(.start(change))
    #expect(first.pending == true)

    await host.seeing()
    let later = await service.answer(.outcome(change))
    #expect(later.outcome == .done && later.pending == false)
    #expect(await host.calls == ["start"])
  }
}
