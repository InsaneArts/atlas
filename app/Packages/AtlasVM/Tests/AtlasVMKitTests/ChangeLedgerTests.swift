import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The change ledger")
struct ChangeLedgerTests {
  let change = ChangeID(String(repeating: "c0", count: 24))!

  @Test("a change is kept as intended, by its run, before it is done, and asked again it is answered from its record")
  func keptAhead() throws {
    let folders = try Folders()
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let intended = ChangeRecord(operation: .start(change), run: "this")

    #expect(try ledger.begin(.start(change), by: "this") == .new)
    #expect(try ledger.unsettled() == [intended])
    #expect(try ledger.begin(.start(change), by: "another") == .asked(intended))

    try ledger.settle(intended, as: .done)
    let settled = ChangeRecord(operation: .start(change), run: "this", outcome: .done)
    #expect(try ledger.begin(.start(change), by: "this") == .asked(settled))
    #expect(try ledger.unsettled().isEmpty)
  }

  @Test("another change asked with an ID already used is refused and never kept")
  func conflicting() throws {
    let folders = try Folders()
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    #expect(try ledger.begin(.start(change), by: "this") == .new)

    #expect(try ledger.begin(.stop(change), by: "this") == .conflicting)
    #expect(try ledger.record(change)?.operation == .start(change))
  }

  @Test("a damaged record, or a link in a record's place, is unreadable, never taken for a change not asked")
  func damaged() throws {
    let folders = try Folders()
    let folder = try folders.folder("changes")
    let ledger = ChangeLedger(folder: folder)
    try Data("{".utf8).write(to: folder.appending(path: "\(change.text).json"))
    #expect(throws: ChangeLedgerError.self) { try ledger.begin(.start(change), by: "this") }

    let linked = ChangeID(String(repeating: "c1", count: 24))!
    try FileManager.default.createSymbolicLink(
      at: folder.appending(path: "\(linked.text).json"),
      withDestinationURL: folders.root.appending(path: "nowhere")
    )
    #expect(throws: ChangeLedgerError.self) { try ledger.begin(.start(linked), by: "this") }
  }
}
