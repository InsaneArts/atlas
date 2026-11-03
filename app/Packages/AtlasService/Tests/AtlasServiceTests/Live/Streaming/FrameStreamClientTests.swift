import Foundation
import Testing

@testable import AtlasService

@Suite("Frame stream client")
struct FrameStreamClientTests {
  /// A client paired with a scripted broker.
  private static func opened(_ script: [ScriptedFramePipeBroker.Answer]) async throws -> (
    FrameStreamClient, ScriptedFramePipeBroker
  ) {
    let broker = ScriptedFramePipeBroker(script: script)
    let (control, frame) = await broker.lanes()
    let client = FrameStreamClient(descriptor: ScriptedFramePipeBroker.descriptor, control: control, frame: frame)
    try await client.open(longest: 1_600)
    return (client, broker)
  }

  @Test("an observer pairs, subscribes, reads exact pictures and commits each")
  func pictures() async throws {
    let (client, broker) = try await Self.opened([])
    for sequence in 1...3 {
      let picture = try await client.next(longest: 1_600)
      #expect(picture.rgb == Data(repeating: UInt8(sequence), count: 24))
      #expect((picture.width, picture.height) == (4, 2))
      #expect(picture.display == ViewerStatus.Size(width: 4, height: 2))
      #expect(picture.revision == "pipe:1:1")
      #expect(try await client.commit(picture))
    }
    let kinds = await broker.kinds
    #expect(kinds.prefix(4) == [.hello, .attach, .pairConfirm, .subscribe])
    #expect(kinds.dropFirst(4).prefix(3) == [.credit, .decoded, .commit])
    await client.close()
  }

  @Test("an expired credit is given again")
  func expiry() async throws {
    let (client, broker) = try await Self.opened([.expire, .expire])
    let picture = try await client.next(longest: 1_600)
    #expect(picture.rgb.count == 24)
    #expect(await broker.kinds.filter { $0 == .credit }.count == 3)
    await client.close()
  }

  @Test("a changed profile is subscribed to again before the next picture")
  func profileChange() async throws {
    let (client, broker) = try await Self.opened([.changeProfile])
    let picture = try await client.next(longest: 1_600)
    #expect(picture.revision == "pipe:2:2")
    #expect(await broker.kinds.filter { $0 == .subscribe }.count == 2)
    await client.close()
  }

  @Test("a picture for another credit, or a refusal, ends the observer")
  func refusals() async throws {
    let (wrong, _) = try await Self.opened([.wrongCredit])
    await #expect(throws: FramePipeError.invalid) { try await wrong.next(longest: 1_600) }
    await wrong.close()
    let (refused, _) = try await Self.opened([.refuse])
    await #expect(throws: ServiceError.self) { try await refused.next(longest: 1_600) }
    await refused.close()
  }

  @Test("a lane that ends while a picture is awaited ends the observer, with no credit given again")
  func endedLane() async throws {
    let (client, broker) = try await Self.opened([.expire])
    await broker.control?.close()
    await #expect(throws: (any Error).self) { try await client.next(longest: 1_600) }
    #expect(await broker.kinds.filter { $0 == .credit }.count <= 1)
    await client.close()
  }

  @Test("an acceptance after its picture, or a state between the picture's pieces, is handled on the way")
  func crossingLanes() async throws {
    let (client, _) = try await Self.opened([.lateAcceptance, .interleaved, .picture])
    for _ in 0..<3 {
      let picture = try await client.next(longest: 1_600)
      #expect(picture.rgb.count == 24)
      #expect(try await client.commit(picture))
    }
    await client.close()
  }

  @Test("a source that stops working ends the observer")
  func unavailable() async throws {
    let (client, _) = try await Self.opened([.unavailable])
    await #expect(throws: ServiceError.self) { try await client.next(longest: 1_600) }
    await client.close()
  }

  @Test("a picture whose commit comes too late is reported, and the next one counts")
  func staleCommit() async throws {
    let (client, _) = try await Self.opened([.staleCommit, .picture])
    let late = try await client.next(longest: 1_600)
    #expect(try await client.commit(late) == false)
    let next = try await client.next(longest: 1_600)
    #expect(try await client.commit(next))
    await client.close()
  }
}
