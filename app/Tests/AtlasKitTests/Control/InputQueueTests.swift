import Testing

@testable import AtlasKit

@MainActor
@Suite("InputQueue")
struct InputQueueTests {
  @Test("a failure from an old lease's request cannot end a new lease")
  func oldFailure() async {
    let sink = RecordingSink()
    sink.holdLease = "old"
    let queue = InputQueue()
    queue.sink = sink
    queue.activate("old")
    queue.enqueue([.key("return")], frame: "frame")
    await settle { sink.hold != nil }
    queue.activate(nil)
    queue.activate("new")
    queue.enqueue([.key("escape")], frame: "frame")
    sink.hold?.resume(throwing: ServiceError("old request disconnected"))
    await settle { sink.sent.count == 2 }
    #expect(sink.sent == ["old", "new"])
    #expect(sink.failures.isEmpty)
    #expect(queue.pending.isEmpty)
  }

  @Test("a newer move replaces a queued one")
  func newerMove() async {
    let sink = RecordingSink()
    sink.holdLease = "lease"
    let queue = InputQueue()
    queue.sink = sink
    queue.activate("lease")
    queue.enqueue([.key("a")], frame: "f")
    await settle { sink.hold != nil }
    queue.enqueue([.move(x: 1, y: 1)], frame: "f")
    queue.enqueue([.move(x: 2, y: 2)], frame: "f")
    #expect(queue.pending.count == 1)
    #expect(queue.pending.first?.actions == [.move(x: 2, y: 2)])
    sink.hold?.resume()
  }

  @Test("more than the queue's limit waiting ends control")
  func limit() async {
    let sink = RecordingSink()
    sink.holdLease = "lease"
    let queue = InputQueue()
    queue.sink = sink
    queue.activate("lease")
    queue.enqueue([.key("a")], frame: "f")
    await settle { sink.hold != nil }
    for index in 0...InputQueue.limit { queue.enqueue([.key("k\(index)")], frame: "f") }
    #expect(sink.failures.count == 1)
    #expect(queue.pending.count == InputQueue.limit)
    sink.hold?.resume()
  }

  @Test("nothing is queued without a lease")
  func noLease() {
    let queue = InputQueue()
    queue.enqueue([.key("a")], frame: "f")
    #expect(queue.pending.isEmpty)
  }
}
