import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The control service")
struct ControlServiceTests {
  let change = ChangeID.repeating("d1")

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
    let reply = try JSONDecoder()
      .decode(ControlReply.self, from: state)
    #expect(reply.power == .running && reply.confirmed == true)
    #expect(try JSONDecoder().decode(ControlReply.self, from: nonsense).ok == false)
  }

  @Test("a window opened for a new VM's setup is opened as one, and a window of before reads back as it was kept")
  func setupWindow() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, _) = try service(host, in: folders)
    let setup = #"{"op":"window","change":"\#(ChangeID.repeating("e1").text)","input":true,"setup":true}"#
    let before = #"{"op":"window","change":"\#(ChangeID.repeating("e2").text)","input":true}"#

    let decoder = JSONDecoder()
    let decoded = try decoder.decode(ControlOperation.self, from: Data(setup.utf8))
    let older = try decoder.decode(ControlOperation.self, from: Data(before.utf8))
    let encoder = JSONEncoder()
    let encoded = try encoder.encode(older)
    let written = try JSONSerialization.jsonObject(with: encoded) as? [String: Any]
    #expect(await service.answer(decoded).outcome == .done)

    #expect(decoded == .window(.repeating("e1"), input: true, setup: true))
    #expect(older == .window(.repeating("e2"), input: true))
    #expect(written?["setup"] == nil, "written as before")
    #expect(await host.setups == [true])
    #expect(ControlHello.ours.capabilities.contains("window-setup"))
  }

  @Test("a state says whether the window is open, as the line its station reads")
  func windowState() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, _) = try service(host, in: folders)

    let closed = await service.answer(.state)
    #expect(await service.answer(.window(change, input: true)).outcome == .done)
    let open = await service.answer(.state)
    let line = await service.answer(line: Data(#"{"op":"state"}"#.utf8))

    #expect(closed.window == .closed)
    #expect(open.window == .open)
    let said = try JSONSerialization.jsonObject(with: line) as? [String: Any]
    #expect(said?["window"] as? String == "open")
  }

  @Test("a change's reply names the run that did it, and its outcome replayed by a later run still names that run")
  func doneBy() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (earlier, ledger) = try service(host, in: folders)
    let opened = await earlier.answer(.window(change, input: true))
    let current = try #require(Incarnation.current())
    let later = Incarnation(
      pid: current.pid,
      startSeconds: current.startSeconds,
      startMicroseconds: current.startMicroseconds,
      id: "a later run"
    )
    let replaying = ControlService(ledger: ledger, host: host, incarnation: later)

    let outcome = await replaying.answer(.outcome(change))
    let again = await replaying.answer(.window(change, input: true))

    let did = try #require(opened.incarnation?.id)
    #expect(opened.by == did)
    #expect(outcome.outcome == .done && outcome.by == did && outcome.incarnation?.id == "a later run")
    #expect(again.outcome == .done && again.by == did)
    #expect(await host.calls == ["openWindow"], "done once")
  }

  @Test("a change voided before it is asked is kept refused, and its request coming later is never done")
  func voidBefore() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, ledger) = try service(host, in: folders)

    let voided = await service.answer(.void(change))
    let late = await service.answer(.window(change, input: true))
    let again = await service.answer(.void(change))
    let outcome = await service.answer(.outcome(change))

    #expect(voided.ok && voided.outcome == ControlService.voided && voided.pending == false)
    #expect(voided.by == voided.incarnation?.id)
    #expect(!late.ok, "a request of a voided change is refused")
    #expect(again.outcome == ControlService.voided)
    #expect(outcome.outcome == ControlService.voided)
    #expect(try ledger.record(change)?.operation == .void(change))
    #expect(await host.calls.isEmpty, "no window opened")
    #expect(await service.answer(.state).window == .closed)
  }

  @Test("a change voided once it was asked is answered as what it came to, naming the run that did it")
  func voidAfter() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, _) = try service(host, in: folders)
    let opened = await service.answer(.window(change, input: true))

    let voided = await service.answer(.void(change))

    #expect(opened.outcome == .done)
    #expect(voided.ok && voided.outcome == .done && voided.by == opened.by)
    #expect(await host.calls == ["openWindow"], "done once")
  }

  @Test("a hello declares that a change can be voided")
  func helloDeclaresVoid() {
    #expect(ControlHello.ours.capabilities.contains("void"))
  }

  @Test("the window's input is off only once its window is seen gone")
  func windowOff() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let (service, _) = try service(host, in: folders)
    #expect(await service.answer(.window(change, input: true)).outcome == .done)
    await host.sticking()

    let off = ChangeID.repeating("e2")
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
    let stopped = await service.answer(.stop(ChangeID.repeating("f4")))
    #expect(stopped.outcome == .done)

    // One that cannot be kept is never done.
    try FileManager.default
      .removeItem(at: ledger.folder)
    let other = ChangeID.repeating("f3")
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
      ChangeID.repeating("a0"),
      ChangeID.repeating("a1"),
      ChangeID.repeating("a2"),
      ChangeID.repeating("a3"),
      ChangeID.repeating("a4")
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

    let other = ChangeID.repeating("b5")
    let refused = await service.answer(.stop(other))
    await host.release()

    #expect(!refused.ok)
    #expect(try ledger.record(other) == nil)
    #expect(await starting.value.outcome == .done)
  }
}
