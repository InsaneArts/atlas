import AsyncSupportTesting
import AtlasControlTesting
import AtlasService
import Testing

@testable import AtlasControl

@MainActor
@Suite("InputQueue")
struct InputQueueTests {
  @Test("a failure from an old lease's request cannot end a new lease")
  func oldFailure() async throws {
    let sink = RecordingSink()
    sink.holdLease = "old"
    let queue = InputQueue()
    queue.sink = sink
    queue.activate("old")
    queue.enqueue([.key("return")], frame: "frame")
    try await settle("the first batch was never sent") { sink.hold != nil }
    queue.activate(nil)
    queue.activate("new")
    queue.enqueue([.key("escape")], frame: "frame")
    sink.hold?.resume(throwing: ServiceError("old request disconnected"))
    try await settle("the new lease never sent its batch") { sink.sent.count == 2 }
    #expect(sink.sent == ["old", "new"])
    #expect(sink.failures.isEmpty)
    #expect(queue.pending.isEmpty)
  }

  @Test("a newer move replaces a queued one")
  func newerMove() async throws {
    let sink = RecordingSink()
    sink.holdLease = "lease"
    let queue = InputQueue()
    queue.sink = sink
    queue.activate("lease")
    queue.enqueue([.key("a")], frame: "f")
    try await settle("the batch was never sent") { sink.hold != nil }
    queue.enqueue([.move(x: 1, y: 1)], frame: "f")
    queue.enqueue([.move(x: 2, y: 2)], frame: "f")
    #expect(queue.pending.count == 1)
    #expect(queue.pending.first?.actions == [.move(x: 2, y: 2)])
    sink.hold?.resume()
  }

  @Test("a scroll waiting to be sent is known, so more scrolling gathers instead of queueing behind it")
  func aWaitingScrollIsKnown() async throws {
    let sink = RecordingSink()
    sink.holdLease = "lease"
    let queue = InputQueue()
    queue.sink = sink
    queue.activate("lease")
    queue.enqueue([.key("a")], frame: "f")
    try await settle("the batch was never sent") { sink.hold != nil }
    #expect(!queue.holdsScroll)
    queue.enqueue([.move(x: 1, y: 1), .scroll(x: 1, y: 1, dx: 0, dy: 3)], frame: "f")
    #expect(queue.holdsScroll)
    sink.hold?.resume()
    // A batch leaves the queue as it is sent, so the scroll sent means it no longer waits.
    try await settle("the scroll was never sent") { sink.sent.count == 2 }
    #expect(!queue.holdsScroll)
  }

  @Test("more than the queue's limit waiting ends control")
  func limit() async throws {
    let sink = RecordingSink()
    sink.holdLease = "lease"
    let queue = InputQueue()
    queue.sink = sink
    queue.activate("lease")
    queue.enqueue([.key("a")], frame: "f")
    try await settle("the batch was never sent") { sink.hold != nil }
    for index in 0...InputQueue.limit { queue.enqueue([.key("k\(index)")], frame: "f") }
    #expect(sink.failures.count == 1)
    #expect(queue.pending.count == InputQueue.limit)
    sink.hold?.resume()
  }

  @Test("settling waits for the batch in flight and every one queued behind it, and returns at once with none")
  func settlingWaitsForEveryBatch() async throws {
    await InputQueue()
      .settled()
    let sink = RecordingSink()
    sink.holdLease = "lease"
    let queue = InputQueue()
    queue.sink = sink
    queue.activate("lease")
    queue.enqueue([.key("a")], frame: "f")
    try await settle("the first batch was never sent") { sink.hold != nil }
    queue.enqueue([.key("b")], frame: "f")
    // Whether, as settling ends, both batches went and the second was answered.
    let settling = Task {
      await queue.settled()
      return sink.sent.count == 2 && sink.hold == nil
    }
    sink.hold?.resume()
    sink.hold = nil
    try await settle("the queued batch was never sent") { sink.hold != nil }
    let second = sink.hold
    sink.hold = nil
    second?.resume()
    #expect(await settling.value, "settling ended only once the second batch was answered")
    #expect(queue.pending.isEmpty)
  }

  @Test("nothing is queued without a lease")
  func noLease() {
    let queue = InputQueue()
    queue.enqueue([.key("a")], frame: "f")
    #expect(queue.pending.isEmpty)
  }
}
