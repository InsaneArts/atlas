import Foundation
import Testing

@testable import AtlasVMKit

@Suite("The picture fence")
struct PictureFenceTests {
  func service(_ host: FakeHost, in folders: Folders) throws -> ControlService {
    let ledger = ChangeLedger(folder: try folders.folder("changes"))
    let incarnation = try #require(Incarnation.current())
    return ControlService(ledger: ledger, host: host, incarnation: incarnation)
  }

  @Test("the fence moves on before the window takes input, and the window says open while it opens")
  func advancesFirst() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    await host.holdingWindow()
    let service = try service(host, in: folders)
    let before = await service.answer(.state)

    let opening = Task { await service.answer(.window(ChangeID.repeating("c1"), input: true)) }
    await host.untilWindowHeld()
    let meanwhile = await service.answer(.state)
    let openMeanwhile = await host.windowIsOpen
    await host.releaseWindow()
    let opened = await opening.value
    let after = await service.answer(.state)

    #expect(before.fence == 0 && before.window == .closed)
    #expect(!openMeanwhile, "the window took no input yet")
    #expect(meanwhile.fence == 1, "moved on while the window was not yet open")
    #expect(meanwhile.window == .open, "an opening under way is never said closed")
    #expect(opened.outcome == .done)
    #expect(after.fence == 1 && after.window == .open)
  }

  @Test("a state after an opening never shows the fence of before, the window closed again or not")
  func neverBefore() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let service = try service(host, in: folders)
    let before = await service.answer(.state)

    #expect(await service.answer(.window(ChangeID.repeating("c2"), input: true)).outcome == .done)
    #expect(await service.answer(.window(ChangeID.repeating("c3"), input: false)).outcome == .done)
    let after = await service.answer(.state)
    #expect(await service.answer(.window(ChangeID.repeating("c4"), input: true)).outcome == .done)
    #expect(await service.answer(.window(ChangeID.repeating("c5"), input: false)).outcome == .done)
    let later = await service.answer(.state)

    #expect(before.window == .closed && after.window == .closed && later.window == .closed)
    #expect(before.fence == 0)
    #expect(after.fence == 1)
    #expect(later.fence == 2)
    #expect(before.incarnation == later.incarnation, "one run's fence")
  }

  @Test("an opening that fails still moves the fence on")
  func failedOpening() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    await host.failing("openWindow")
    let service = try service(host, in: folders)

    let refused = await service.answer(.window(ChangeID.repeating("c6"), input: true))
    let after = await service.answer(.state)

    #expect(refused.outcome != .done)
    #expect(after.fence == 1 && after.window == .closed)
  }

  @Test("a state's line carries the fence its station reads")
  func onTheLine() async throws {
    let folders = try Folders()
    let service = try service(FakeHost(power: .running), in: folders)

    let line = await service.answer(line: Data(#"{"op":"state"}"#.utf8))

    let said = try JSONSerialization.jsonObject(with: line) as? [String: Any]
    #expect(said?["fence"] as? Int == 0)
    #expect(said?["window"] as? String == "closed")
  }

  /// Whether a station takes a picture between the states `before` and
  /// `after`: the same run and fence, the window closed both times.
  func admits(_ before: ControlReply, _ after: ControlReply) -> Bool {
    before.window == .closed && after.window == .closed && before.fence == after.fence
      && before.incarnation == after.incarnation
  }

  @Test("a minimized window is open, restored and minimized again, until it is closed")
  func minimized() async throws {
    let folders = try Folders()
    let host = FakeHost(power: .running)
    let service = try service(host, in: folders)
    let closed = await service.answer(.state)
    #expect(await service.answer(.window(ChangeID.repeating("d1"), input: true)).outcome == .done)

    await host.minimizeWindow()
    let minimized = await service.answer(.state)
    await host.restoreWindow()
    let restored = await service.answer(.state)
    await host.minimizeWindow()
    let again = await service.answer(.state)
    #expect(await service.answer(.window(ChangeID.repeating("d2"), input: false)).outcome == .done)
    let shut = await service.answer(.state)

    for state in [minimized, restored, again] {
      #expect(state.window == .open)
      #expect(state.fence == 1, "a restore moves nothing, and needs not")
    }
    #expect(!admits(closed, minimized))
    #expect(!admits(minimized, again), "no picture spans the restore")
    #expect(!admits(closed, again))
    #expect(shut.window == .closed && shut.fence == 1)
    #expect(!admits(closed, shut), "the opening before the minimizing still shows")
  }

  @Test("a window is open while it is visible or minimized, and closed only when neither")
  func windowStates() {
    #expect(WindowState(visible: true, minimized: false) == .open)
    #expect(WindowState(visible: false, minimized: true) == .open)
    #expect(WindowState(visible: true, minimized: true) == .open)
    #expect(WindowState(visible: false, minimized: false) == .closed)
  }

  @Test("a window seen closed after an opening began since the fence was read is said open")
  func openingSinceReading() {
    var fence = PictureFence()
    let reading = fence

    fence.beginOpening()
    #expect(fence.mayHaveOpened(since: reading))
    fence.endOpening()
    #expect(fence.mayHaveOpened(since: reading))
    #expect(!fence.mayHaveOpened(since: fence))
    #expect(fence.value == 1)
  }
}
