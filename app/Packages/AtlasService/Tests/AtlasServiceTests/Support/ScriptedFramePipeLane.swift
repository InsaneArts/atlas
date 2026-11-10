import Foundation
import os

@testable import AtlasService

/// One lane to a ``ScriptedFramePipeBroker``: what the client writes goes to the broker, and what the broker answers
/// on this lane is read back in order. The broker can wait until the client has handled all it sent, to order what
/// comes next after it without a pause.
final class ScriptedFramePipeLane: FramePipeTransport, Sendable {
  /// What the lane sent and what of it the client read, and who waits for the two to meet.
  private struct Delivery {
    /// Chunks sent.
    var sent = 0
    /// Reads the client began: each after it handled the chunk before.
    var asked = 0
    /// Waiting until everything sent is handled.
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

  /// Returns once the client has handled everything this lane sent so far: it read the last chunk, and asked for the
  /// next, so whatever it decoded from it is already passed on.
  func drained() async {
    await withCheckedContinuation { continuation in
      let caughtUp = delivery.withLock { delivery in
        guard delivery.asked <= delivery.sent else { return true }
        delivery.waiting.append(continuation)
        return false
      }
      if caughtUp { continuation.resume() }
    }
  }

  func read(upTo limit: Int) async throws -> Data {
    let caughtUp = delivery.withLock { delivery in
      delivery.asked += 1
      guard delivery.asked > delivery.sent else { return [CheckedContinuation<Void, Never>]() }
      defer { delivery.waiting = [] }
      return delivery.waiting
    }
    for waiter in caughtUp { waiter.resume() }
    var iterator = replies.makeAsyncIterator()
    return await iterator.next() ?? Data()
  }

  func write(_ data: Data) async throws {
    await broker.received(data, on: lane)
  }

  func close() {
    sink.finish()
  }
}
