import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A VM's removal asked of its runner")
struct ForgetTests {
  @Test("once forget is asked no other change is taken, and forget asked again is answered from its record")
  func forgetHoldsEveryOtherChange() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))

    let forgotten = await service.answer(.forget(.repeating("f0")))
    let started = await service.answer(.start(.repeating("f1")))
    let again = await service.answer(.forget(.repeating("f0")))

    #expect(forgotten.outcome == .done)
    #expect(started.outcome == nil && started.ok == false)
    #expect(again.outcome == .done)
    #expect(await host.forgotten == 1)
  }

  @Test("a VM's lifecycle lock and removal record are kept outside its folder")
  func keptOutsideTheFolder() throws {
    let parent = URL(filePath: "/tmp/VMs", directoryHint: .isDirectory)
    let desktop = String(repeating: "ab", count: 12)
    let folder = try #require(VMFolder(in: parent, desktop: desktop))

    #expect(folder.lifecycleLock.path(percentEncoded: false) == "/tmp/VMs/.locks/\(desktop).lock")
    #expect(folder.removal.path(percentEncoded: false) == "/tmp/VMs/.removals/\(desktop).json")
    #expect(!folder.lifecycleLock.path(percentEncoded: false).hasPrefix(folder.root.path(percentEncoded: false)))
  }
}
