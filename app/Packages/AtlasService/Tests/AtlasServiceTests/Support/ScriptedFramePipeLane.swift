import Foundation
import os

@testable import AtlasService

/// One lane to a ``ScriptedFramePipeBroker``: what the client writes goes to the broker, and what the broker answers
/// on this lane is read back in order. The broker can wait until the client has read all it sent, to order what
/// comes next after it without a pause.
final class ScriptedFramePipeLane: FramePipeTransport, Sendable {
  /// What the lane sent and what of it the client read, and who waits for the two to meet.
  private struct Delivery {
    /// Chunks sent.
    var sent = 0
    /// Chunks read.
    var read = 0
    /// Waiting until everything sent is read.
    var waiting: [CheckedContinuation<Void, Never>] = []
  }

  /// The lane's kind.
  let lane: FramePipeLane
  private let broker: ScriptedFramePipeBroker
  private let replies: AsyncStream<Data>
  private let sink: AsyncStream<Data>.Continuation
  /// The delivery, read and written only under its lock. `OSAllocatedUnfairLock`, as `Mutex` needs macOS 15.
  private let delivery = OSAllocatedUnfairLock(initialState: Delivery())

  init(_ lane: FramePipeLane, broker: ScriptedFramePipeBroker) {
    self.lane = lane
    self.broker = broker
    (replies, sink) = AsyncStream<Data>.makeStream()
  }

  /// Answers `data` on this lane, for the client's next read.
  ///
  /// - Parameter data: The bytes.
  func send(_ data: Data) {
    delivery.withLock { $0.sent += 1 }
    sink.yield(data)
  }

  /// Returns once the client has read everything this lane sent so far.
  func drained() async {
    await withCheckedContinuation { continuation in
      let caughtUp = delivery.withLock { delivery in
        guard delivery.read < delivery.sent else { return true }
        delivery.waiting.append(continuation)
        return false
      }
      if caughtUp { continuation.resume() }
    }
  }

  func read(upTo limit: Int) async throws -> Data {
    var iterator = replies.makeAsyncIterator()
    let data = await iterator.next() ?? Data()
    let caughtUp = delivery.withLock { delivery in
      delivery.read += 1
      guard delivery.read >= delivery.sent else { return [CheckedContinuation<Void, Never>]() }
      defer { delivery.waiting = [] }
      return delivery.waiting
    }
    for waiter in caughtUp { waiter.resume() }
    return data
  }

  func write(_ data: Data) async throws {
    await broker.received(data, on: lane)
  }

  func close() {
    sink.finish()
  }
}
