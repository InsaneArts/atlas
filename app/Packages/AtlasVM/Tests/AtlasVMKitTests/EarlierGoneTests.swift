import Foundation
import Testing

@testable import AtlasVMKit

@Suite("Whether every earlier run's VM is gone, as a state says it")
struct EarlierGoneTests {
  func service(_ host: FakeHost, in folders: Folders) throws -> ControlService {
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    return ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
  }

  @Test("a state says every earlier run's VM is gone only once this run recovered and sees them gone")
  func onceRecovered() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let service = try service(host, in: folders)
    let lock = try #require(RunnerLock(at: folders.root.appending(path: "Runner.lock")))
    await host.earlier(gone: false)

    let before = await service.answer(.state)
    await service.recover(holding: lock, vmGone: false)
    let unseen = await service.answer(.state)
    await host.earlier(gone: true)
    let seen = await service.answer(.state)

    #expect(before.earlierGone == false)
    #expect(unseen.earlierGone == false)
    #expect(seen.earlierGone == true && seen.confirmed == true)
  }

  @Test("a state from a run that has not recovered never says earlier runs are gone")
  func notRecovered() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let service = try service(host, in: folders)
    await host.earlier(gone: true)

    let state = await service.answer(.state)
    let line = await service.answer(line: Data(#"{"op":"state"}"#.utf8))

    #expect(state.earlierGone == false)
    let said = try JSONSerialization.jsonObject(with: line) as? [String: Any]
    #expect(said?["earlierGone"] as? Bool == false)
  }
}
