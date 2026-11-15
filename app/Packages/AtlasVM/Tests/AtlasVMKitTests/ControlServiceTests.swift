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

  @Test(
    "under the runner's lock, earlier runs' changes are settled by what is there, a window's only once its VM is gone"
  )
  func recovery() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .stopped)
    let (service, ledger) = try service(host, in: folders)
    let lock = try #require(RunnerLock(at: folders.root.appending(path: "Runner.lock")))
    let (create, start, on, off, own) = (
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
      #expect(try ledger.begin(operation, by: "an earlier run") == .new)
    }
    // This very run's own change is never an earlier run's.
    let this = try #require(await service.answer(.hello).incarnation?.id)
    #expect(try ledger.begin(.stop(own), by: this) == .new)

    // The VM not seen gone: the window's changes wait.
    await service.recover(holding: lock, vmGone: false)
    #expect(try ledger.record(create)?.outcome == .done)
    #expect(try ledger.record(start)?.outcome != .done, "its VM is stopped")
    #expect(try ledger.record(on)?.outcome == nil)
    #expect(try ledger.record(off)?.outcome == nil)

    await service.recover(holding: lock, vmGone: true)
    #expect(try ledger.record(on)?.outcome != .done)
    #expect(try ledger.record(off)?.outcome == .done)
    #expect(try ledger.record(own)?.outcome == nil, "this run's change is its own to settle")
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
