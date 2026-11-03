import Foundation
import Testing

@testable import AtlasService

@Suite("Frame packet codec")
struct FramePacketCodecTests {
  /// A prefix with each field given, for refusals the encoder would never write.
  private static func prefix(
    magic: String = "ATFP",
    major: UInt16 = 1,
    kind: UInt16 = 9,
    flags: UInt32 = 0,
    metadata: UInt32 = 2,
    payload: UInt64 = 0,
    request: UInt64 = 1
  ) -> Data {
    var bytes = Data(magic.utf8)
    for (value, width) in [
      (UInt64(major), 2), (UInt64(kind), 2), (UInt64(flags), 4), (UInt64(metadata), 4), (payload, 8), (request, 8),
    ] {
      bytes.append(contentsOf: (0..<width).reversed().map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) })
    }
    return bytes
  }

  /// The first refusal decoding `bytes` on `lane` meets, or nil.
  private static func refusal(_ bytes: Data, lane: FramePipeLane = .control) -> FramePipeError? {
    do {
      _ = try FramePipeFixture.packets(lane: lane, bytes: bytes, step: bytes.count)
      return nil
    } catch {
      return error
    }
  }

  @Test("an encoded packet decodes back whole, however the reads split or join it")
  func roundTrip() throws {
    let first = try FramePacketCodec.encode(
      kind: .credit,
      request: 7,
      metadata: Data(#"{"creditID":1}"#.utf8),
      payloadLength: 0
    )
    let pixels = Data(repeating: 9, count: 3 * 200 * 200)
    let second =
      try FramePacketCodec.encode(
        kind: .fullFrame,
        request: 8,
        metadata: Data("{}".utf8),
        payloadLength: UInt64(pixels.count)
      )
      + pixels
    #expect(first.count == FramePipeLimits.prefix + 14)
    for step in [1, 5, 31, 32, 33, first.count + second.count] {
      let control = try FramePipeFixture.packets(lane: .control, bytes: first + first, step: step)
      #expect(control.map(\.header.request) == [7, 7])
      #expect(control.allSatisfy { $0.metadata == Data(#"{"creditID":1}"#.utf8) && $0.payload.isEmpty })
      let frame = try FramePipeFixture.packets(lane: .frame, bytes: second, step: step)
      #expect(frame.first?.payload == pixels)
    }
  }

  @Test("a payload comes out in pieces no larger than a chunk")
  func chunks() throws {
    let pixels = Data(repeating: 1, count: FramePipeLimits.chunk * 2 + 3)
    var input =
      try FramePacketCodec.encode(
        kind: .fullFrame,
        request: 1,
        metadata: Data("{}".utf8),
        payloadLength: UInt64(pixels.count)
      )
      + pixels
    var codec = FramePacketCodec(lane: .frame)
    var pieces: [Int] = []
    while let item = try codec.next(&input) {
      if case .payload(let piece) = item {
        pieces.append(piece.count)
      }
    }
    #expect(pieces == [FramePipeLimits.chunk, FramePipeLimits.chunk, 3])
    try codec.end()
  }

  @Test("a prefix that is not this protocol is refused before anything is reserved")
  func refusedPrefixes() {
    let body = Data("{}".utf8)
    #expect(Self.refusal(Self.prefix(magic: "ATFQ") + body) == .invalid)
    #expect(Self.refusal(Self.prefix(major: 2) + body) == .unsupportedVersion)
    #expect(Self.refusal(Self.prefix(kind: 0) + body) == .invalid)
    #expect(Self.refusal(Self.prefix(kind: 51) + body) == .invalid)
    #expect(Self.refusal(Self.prefix(flags: 1) + body) == .invalid)
    #expect(Self.refusal(Self.prefix(kind: 11, payload: 3) + body) == .invalid, "a picture on the control lane")
    #expect(Self.refusal(Self.prefix(metadata: 1) + body) == .invalid)
    #expect(Self.refusal(Self.prefix(metadata: UInt32.max)) == .oversized)
    #expect(Self.refusal(Self.prefix(payload: 1) + body) == .invalid, "a control packet carries no payload")
    #expect(Self.refusal(Self.prefix(kind: 11, payload: FramePipeLimits.payload + 1), lane: .frame) == .oversized)
    #expect(Self.refusal(Self.prefix(kind: 11, payload: 0) + body, lane: .frame) == .invalid)
    #expect(Self.refusal(Self.prefix(request: 0) + body) == .invalid, "a credit answers a request")
    #expect(Self.refusal(Self.prefix(kind: 37, request: 3) + body) == .invalid, "a state change answers none")
    #expect(Self.refusal(Self.prefix(kind: 37, request: 0) + body) == nil)
  }

  @Test("an end inside a packet is truncation, wherever it falls")
  func truncation() throws {
    let packet =
      try FramePacketCodec.encode(kind: .fullFrame, request: 1, metadata: Data("{}".utf8), payloadLength: 3)
      + Data([1, 2, 3])
    for cut in 1..<packet.count {
      #expect(Self.refusal(packet.prefix(cut), lane: .frame) == .truncated, "\(cut)")
    }
    #expect(Self.refusal(packet, lane: .frame) == nil)
  }

  @Test("the encoder refuses what the decoder would")
  func encoderBounds() {
    let tooLong = Data(repeating: 0x20, count: FramePipeLimits.metadata + 1)
    #expect(throws: FramePipeError.oversized) {
      try FramePacketCodec.encode(kind: .credit, request: 1, metadata: tooLong, payloadLength: 0)
    }
    #expect(throws: FramePipeError.invalid) {
      try FramePacketCodec.encode(kind: .credit, request: 1, metadata: Data("{}".utf8), payloadLength: 1)
    }
    #expect(throws: FramePipeError.invalid) {
      try FramePacketCodec.encode(kind: .credit, request: 0, metadata: Data("{}".utf8), payloadLength: 0)
    }
    #expect(throws: FramePipeError.oversized) {
      try FramePacketCodec.encode(
        kind: .fullFrame,
        request: 1,
        metadata: Data("{}".utf8),
        payloadLength: FramePipeLimits.payload + 1
      )
    }
  }
}
