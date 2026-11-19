import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The administrator's password kept to the window")
struct AdminCanaryTests {
  let canary = "bcdefghjknprstuvbcde"

  @Test("a setup window's password shows only in its row: no reply, record or state of Atlas VM's carries it")
  func neverLeaves() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    await host.setAdminPassword(canary)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let incarnation = try #require(Incarnation.current())
    let service = ControlService(ledger: ledger, host: host, incarnation: incarnation)
    let on = ChangeID.repeating("f1")
    let opening = #"{"op":"window","change":"\#(on.text)","input":true,"setup":true}"#
    let asked = [
      opening, #"{"op":"state"}"#, #"{"op":"hello"}"#,
      #"{"op":"outcome","change":"\#(on.text)"}"#,
    ]

    var said = Data()
    for line in asked {
      said.append(await service.answer(line: Data(line.utf8)))
    }
    var hint = try #require(await host.shownHint)
    hint.toggle()

    #expect(hint.shown.joined() == canary, "its row shows it once revealed")
    #expect(!String(decoding: said, as: UTF8.self).contains(canary))
    let root = try folders.folder("changes")
    let manager = FileManager.default
    let kept = try manager.subpathsOfDirectory(atPath: root.path(percentEncoded: false))
    for name in kept {
      let bytes = (try? Data(contentsOf: root.appending(path: name))) ?? Data()
      #expect(!String(decoding: bytes, as: UTF8.self).contains(canary), "\(name)")
    }
    #expect(!kept.isEmpty, "the window's change was kept")
    #expect(!"\(hint)".contains(canary))
  }
}
