import Darwin
import Foundation
import Testing

@testable import AtlasVMKit

@Suite("How much of a VM's disk is used")
struct DiskUseTests {
  @Test("the state names the disk's used bytes")
  func stateNamesUse() async throws {
    let folders = try Folders()
    let disk = try folders.folder("vm")
      .appending(path: "Disk.asif")
    // A sparse file: far larger than what it takes.
    #expect(
      FileManager.default
        .createFile(atPath: disk.path(percentEncoded: false), contents: nil)
    )
    let handle = try FileHandle(forWritingTo: disk)
    try handle.write(contentsOf: Data(repeating: 7, count: 1 << 20))
    try handle.truncate(atOffset: 1 << 32)
    try handle.close()
    var status = stat()
    #expect(lstat(disk.path(percentEncoded: false), &status) == 0)
    let host = FakeHost(power: .stopped)
    await host.disk(at: disk)
    let service = ControlService(
      ledger: ChangeLedger(folder: try folders.folder("changes")),
      host: host,
      incarnation: try #require(Incarnation.current())
    )

    let reply = await service.answer(.state)

    let used = try #require(reply.disk?.used)
    #expect(used == UInt64(status.st_blocks) * 512)
    #expect(used >= 1 << 20 && used < 1 << 32, "what it takes, not its length")
    #expect(reply.disk?.size == 128_000_000_000)
  }

  @Test("a disk not made yet says its size and no use")
  func notMade() throws {
    let folders = try Folders()
    let disk = try folders.folder("vm")
      .appending(path: "Disk.asif")

    let use = DiskUse.of(disk, size: 10)

    #expect(use == DiskUse(used: nil, size: 10))
  }
}
