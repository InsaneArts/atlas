import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The control service")
struct ControlServiceTests {
  let change = ChangeID(String(repeating: "d1", count: 24))!

  func service(_ host: FakeHost, in folders: Folders) throws -> (ControlService, ChangeLedger) {
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let incarnation = try #require(Incarnation.current())
    return (ControlService(ledger: ledger, host: host, incarnation: incarnation), ledger)
  }

  @Test("a change is done once, and asked again it is answered from its record without being done again")
  func once() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let (service, _) = try service(host, in: folders)

    let first = await service.answer(.start(change))
    let again = await service.answer(.start(change))

    #expect(first.ok && first.outcome == .done && first.pending == false)
    #expect(again.ok && again.outcome == .done)
    #expect(await host.calls == ["start"])
  }

  @Test("a hello declares the runner's version and capabilities, and names this run")
  func hello() async throws {
    let folders = try Folders()
    let (service, _) = try service(FakeHost(), in: folders)

    let reply = await service.answer(.hello)

    #expect(reply.hello == .ours)
    #expect(reply.incarnation?.pid == getpid())
  }

  @Test("a request line is answered with one reply line, and a line that is no request is refused")
  func lines() async throws {
    let folders = try Folders()
    let (service, _) = try service(FakeHost(power: .running), in: folders)

    let state = await service.answer(line: Data(#"{"op":"state"}"#.utf8))
    let nonsense = await service.answer(line: Data(#"{"op":"format"}"#.utf8))

    #expect(state.last == UInt8(ascii: "\n"))
    let reply = try JSONDecoder().decode(ControlReply.self, from: state)
    #expect(reply.power == .running && reply.confirmed == true)
    #expect(try JSONDecoder().decode(ControlReply.self, from: nonsense).ok == false)
  }

  @Test("the window's input is off only once its window is seen gone")
  func windowOff() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, _) = try service(host, in: folders)
    #expect(await service.answer(.window(change, input: true)).outcome == .done)
    await host.sticking()

    let off = ChangeID(String(repeating: "e2", count: 24))!
    let reply = await service.answer(.window(off, input: false))

    #expect(reply.outcome == .refused("It did not take effect."))
  }

  @Test("a change that failed is told by what is there: done if it took effect, refused if not")
  func failures() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    await host.failing("start")
    let (service, ledger) = try service(host, in: folders)

    // The fake starts nothing when it fails: the VM is seen stopped.
    let failed = await service.answer(.start(change))
    guard case .refused = failed.outcome else {
      Issue.record("a start that failed and did nothing came to \(String(describing: failed.outcome))")
      return
    }

    // One that failed after it took effect is done.
    await host.failing(nil, after: "stop")
    let stopped = await service.answer(.stop(ChangeID(String(repeating: "f4", count: 24))!))
    #expect(stopped.outcome == .done)

    // One that cannot be kept is never done.
    try FileManager.default.removeItem(at: ledger.folder)
    let other = ChangeID(String(repeating: "f3", count: 24))!
    let refused = await service.answer(.stop(other))
    #expect(!refused.ok)
    #expect(await host.calls == ["start", "stop"])
  }

  @Test("an earlier run's own changes wait while it may still run, and are settled by what is there once it is gone")
  func recovery() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let (service, ledger) = try service(host, in: folders)
    let gone = Incarnation(pid: 0x7FFF_FFF0, startSeconds: 1, startMicroseconds: 0, id: "gone")
    let (create, start, on, off, others) = (
      ChangeID(String(repeating: "a0", count: 24))!,
      ChangeID(String(repeating: "a1", count: 24))!,
      ChangeID(String(repeating: "a2", count: 24))!,
      ChangeID(String(repeating: "a3", count: 24))!,
      ChangeID(String(repeating: "a4", count: 24))!
    )
    let asked: [ControlOperation] = [
      .create(create), .start(start), .window(on, input: true), .window(off, input: false),
    ]
    for operation in asked {
      #expect(try ledger.begin(operation, by: gone.id) == .new)
    }
    #expect(try ledger.begin(.stop(others), by: "another run") == .new)

    // With no earlier run known, or this very run named, nothing is settled.
    await service.recover(earlier: nil, vmGone: true)
    await service.recover(earlier: Incarnation.current(), vmGone: true)
    #expect(try ledger.unsettled().count == 5)

    // Gone, but its VM not seen gone: the window's changes wait.
    await service.recover(earlier: gone, vmGone: false)
    #expect(try ledger.record(create)?.outcome == .done)
    #expect(try ledger.record(start)?.outcome != .done, "its VM is stopped")
    let waiting = try ledger.unsettled().map(\.operation)
    #expect(waiting == [.window(on, input: true), .window(off, input: false), .stop(others)])

    await service.recover(earlier: gone, vmGone: true)
    #expect(try ledger.record(on)?.outcome != .done)
    #expect(try ledger.record(off)?.outcome == .done)
    #expect(try ledger.record(others)?.outcome == nil, "another run's change is never settled")
    #expect(await host.calls.isEmpty, "nothing was done again")
  }

  @Test("a VM is made only when it is seen absent just now")
  func createNeedsAbsence() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .absent, confirmed: false)
    let (service, _) = try service(host, in: folders)

    let reply = await service.answer(.create(change))

    #expect(reply.outcome != .done)
    #expect(await host.calls.isEmpty)
  }

  @Test("one change is done at a time; a second asked meanwhile is refused unkept")
  func oneAtATime() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    await host.holding()
    let (service, ledger) = try service(host, in: folders)
    let starting = Task { await service.answer(.start(change)) }
    await host.untilHeld()

    let other = ChangeID(String(repeating: "b5", count: 24))!
    let refused = await service.answer(.stop(other))
    await host.release()

    #expect(!refused.ok)
    #expect(try ledger.record(other) == nil)
    #expect(await starting.value.outcome == .done)
  }
}
