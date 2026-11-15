import Foundation
import Testing

@testable import AtlasVMKit

@Suite("A run's earlier runs")
struct EarlierRunsTests {
  @Test(
    "an earlier run is kept while its VM may run, dropped once its VM is gone, and a VM still running is never dropped"
  )
  func unresolved() throws {
    let run = try #require(Incarnation.current())
    let ended = Process()
    ended.executableURL = URL(filePath: "/usr/bin/true")
    try ended.run()
    let gone = try #require(MachineProcess(pid: ended.processIdentifier))
    ended.waitUntilExit()
    let alive = try #require(MachineProcess(pid: getpid()))
    let earlier = [RunRecord(run: run, machine: gone), RunRecord(run: run, machine: alive)]

    let record = RunRecord(run: run, machine: gone, earlier: earlier)

    #expect(record.unresolved() == [RunRecord(run: run, machine: alive)])
    #expect(RunRecord(run: run, machine: gone).unresolved().isEmpty)
  }
}
