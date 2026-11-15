import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A run's record")
struct RunRecordTests {
  @Test("a run's record is read back as it was kept, and none is read where none was kept")
  func kept() throws {
    let folders = try Folders()
    let file = folders.root.appending(path: "Incarnation.json")
    #expect(try RunRecord.read(from: file) == nil)
    let record = RunRecord(run: try #require(Incarnation.current()), machine: MachineProcess(pid: getpid()))

    try record.write(to: file)

    #expect(try RunRecord.read(from: file) == record)
  }

  @Test("a VM's process kept and since exited is gone, and one still running is not")
  func machineGone() throws {
    let run = try #require(Incarnation.current())
    let ended = Process()
    ended.executableURL = URL(filePath: "/usr/bin/true")
    try ended.run()
    let process = try #require(MachineProcess(pid: ended.processIdentifier))
    ended.waitUntilExit()

    #expect(RunRecord(run: run, machine: process).machineGone())
    #expect(!RunRecord(run: run, machine: MachineProcess(pid: getpid())).machineGone())
  }
}
