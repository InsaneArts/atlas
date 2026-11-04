import Foundation

@testable import AtlasService

/// One lane to a ``ScriptedFramePipeBroker``: what the client writes goes to the broker, and what the broker answers
/// on this lane is read back in order.
final class ScriptedFramePipeLane: FramePipeTransport, Sendable {
  /// The lane's kind.
  let lane: FramePipeLane
  private let broker: ScriptedFramePipeBroker
  private let replies: AsyncStream<Data>
  let sink: AsyncStream<Data>.Continuation

  init(_ lane: FramePipeLane, broker: ScriptedFramePipeBroker) {
    self.lane = lane
    self.broker = broker
    (replies, sink) = AsyncStream<Data>.makeStream()
  }

  func read(upTo limit: Int) async throws -> Data {
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
