import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The limit on VMs running at once")
struct VMLimitTests {
  @Test("a VM counts as running while its run record names a process that runs, or cannot be read")
  func counted() throws {
    let folders = try Folders()
    let parent = try folders.folder("VMs")
    let run = try #require(Incarnation.current())
    // A child that waits on a pipe, so its identity is taken while it runs,
    // then let go and seen gone.
    let release = Pipe()
    let ended = Process()
    ended.executableURL = URL(filePath: "/bin/cat")
    ended.standardInput = release
    ended.standardOutput = FileHandle.nullDevice
    try ended.run()
    let gone = try #require(MachineProcess(pid: ended.processIdentifier))
    try release.fileHandleForWriting.close()
    ended.waitUntilExit()
    let (running, stopped, unread, never) = (
      try #require(VMFolder(in: parent, desktop: String(repeating: "a1", count: 12))),
      try #require(VMFolder(in: parent, desktop: String(repeating: "b2", count: 12))),
      try #require(VMFolder(in: parent, desktop: String(repeating: "c3", count: 12))),
      try #require(VMFolder(in: parent, desktop: String(repeating: "d4", count: 12)))
    )
    for folder in [running, stopped, unread, never] {
      try VMFolder.makePrivate(folder.root)
    }
    let alive = RunRecord(run: run, machine: MachineProcess(pid: getpid()))
    try alive.write(to: running.incarnation)
    let exited = RunRecord(run: run, machine: gone)
    try exited.write(to: stopped.incarnation)
    try PrivateFile.write(Data("not a record".utf8), to: unread.incarnation)

    #expect(VMLimit.running(among: parent) == 2)
    #expect(VMLimit.running(among: parent, besides: running.root) == 1)
    #expect(VMLimit.reached == "This Mac already runs 2 macOS VMs, as many as it may.")
  }

  @Test("a start refused for the limit comes to that refusal in its words, as the line its station reads")
  func startRefused() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    await host.refusingStarts(VMLimit.reached)
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let service = ControlService(ledger: ledger, host: host, incarnation: try #require(Incarnation.current()))
    let change = ChangeID.repeating("e5")

    let started = await service.answer(line: Data(#"{"op":"start","change":"\#(change.text)"}"#.utf8))
    let outcome = await service.answer(line: Data(#"{"op":"outcome","change":"\#(change.text)"}"#.utf8))
    let never = ChangeID.repeating("e6")
    let unknown = await service.answer(line: Data(#"{"op":"outcome","change":"\#(never.text)"}"#.utf8))

    for line in [started, outcome] {
      let said = try #require(try JSONSerialization.jsonObject(with: line) as? [String: Any])
      let came = try #require(said["outcome"] as? [String: Any])
      #expect(said["ok"] as? Bool == true)
      #expect(said["change"] as? String == change.text)
      #expect(said["pending"] as? Bool == false)
      #expect(came["state"] as? String == "refused")
      #expect(came["reason"] as? String == VMLimit.reached)
    }
    let said = try #require(try JSONSerialization.jsonObject(with: unknown) as? [String: Any])
    #expect(said["ok"] as? Bool == true && said["change"] as? String == never.text)
    #expect(said["outcome"] == nil && said["pending"] == nil, "a change never asked is neither")
    #expect(await host.calls == ["start"])
  }
}
