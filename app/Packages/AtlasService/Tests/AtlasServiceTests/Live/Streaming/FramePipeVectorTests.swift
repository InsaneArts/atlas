import Foundation
import Testing

@testable import AtlasService

@Suite("Frame pipe golden vectors")
struct FramePipeVectorTests {
  @Test("every golden vector decodes as its manifest says at every split")
  func everyVector() throws {
    let manifest = try FramePipeFixture.manifest()
    #expect(manifest.count == 10)
    let fullVector = try #require(manifest.first { $0["file"] as? String == "full-4x4.bin" })
    let fullPacket = try #require((fullVector["packets"] as? [[String: Any]])?.first)
    #expect(fullPacket["kind"] as? Int == Int(FramePacketKind.fullFrame.rawValue))
    let full = FramePipeFixture.unhex(try #require(fullPacket["payload"] as? String))
    for vector in manifest {
      let name = try #require(vector["file"] as? String)
      let bytes = try Data(contentsOf: FramePipeFixture.folder().appending(path: name))
      #expect(bytes.count == vector["bytes"] as? Int, "\(name)")
      let lane: FramePipeLane = vector["lane"] as? String == "control" ? .control : .frame
      for step in [1, 7, 32, max(bytes.count, 1)] {
        let decoded = Result { () throws(FramePipeError) in
          try FramePipeFixture.packets(lane: lane, bytes: bytes, step: step)
        }
        switch vector["result"] as? String {
        case "truncated":
          #expect(throws: FramePipeError.truncated) { try decoded.get() }
        case "oversized":
          #expect(throws: FramePipeError.oversized) { try decoded.get() }
        case "invalid-metadata":
          let packet = try #require(try decoded.get().first)
          #expect(packet.header.kind == .fullFrame)
          #expect(throws: FramePipeError.invalid) { try FramePictureMetadata(validating: packet.metadata) }
        default:
          let expected = try #require(vector["packets"] as? [[String: Any]])
          let packets = try decoded.get()
          #expect(packets.count == expected.count, "\(name)")
          for (packet, expected) in zip(packets, expected) {
            try Self.check(packet, against: expected, full: full, name: name)
          }
          if step == 1 {
            try Self.everyCut(bytes, lane: lane, matches: packets, name: name)
          }
        }
      }
    }
  }

  /// Checks that `bytes` read in two pieces, cut anywhere, decode to `packets`.
  private static func everyCut(_ bytes: Data, lane: FramePipeLane, matches packets: [DecodedFramePacket], name: String)
    throws
  {
    for cut in 1..<bytes.count {
      var codec = FramePacketCodec(lane: lane)
      var found: [DecodedFramePacket] = []
      for var input in [bytes.prefix(cut), bytes.dropFirst(cut)] {
        while let item = try codec.next(&input) {
          switch item {
          case .packet(let header, let metadata): found.append(DecodedFramePacket(header: header, metadata: metadata))
          case .payload(let piece): found[found.count - 1].payload.append(piece)
          }
        }
      }
      try codec.end()
      #expect(found.map(\.header) == packets.map(\.header), "\(name) cut at \(cut)")
      #expect(found.map(\.metadata) == packets.map(\.metadata), "\(name) cut at \(cut)")
      #expect(found.map(\.payload) == packets.map(\.payload), "\(name) cut at \(cut)")
    }
  }

  /// Checks `packet` against its manifest entry: kind, request, payload and metadata, and a delta's rectangles and
  /// the picture they make.
  private static func check(
    _ packet: DecodedFramePacket,
    against expected: [String: Any],
    full: Data,
    name: String
  ) throws {
    #expect(Int(packet.header.kind.rawValue) == expected["kind"] as? Int, "\(name)")
    #expect(packet.header.request == expected["request"] as? UInt64, "\(name)")
    #expect(packet.payload == FramePipeFixture.unhex(try #require(expected["payload"] as? String)), "\(name)")
    let manifestMetadata = try JSONSerialization.data(withJSONObject: try #require(expected["metadata"]))
    guard packet.header.kind.hasPayload || packet.header.kind == .unchangedFrame else {
      _ = try FrameJSONValidator.validate(packet.metadata)
      let decoded = try JSONSerialization.jsonObject(with: packet.metadata) as? NSDictionary
      #expect(decoded == (expected["metadata"] as? NSDictionary), "\(name)")
      return
    }
    let metadata = try FramePictureMetadata(validating: packet.metadata)
    #expect(metadata == (try FramePictureMetadata(validating: manifestMetadata)), "\(name)")
    try metadata.check(kind: packet.header.kind, payloadLength: UInt64(packet.payload.count))
    guard packet.header.kind == .deltaFrame else { return }
    let records = Int(metadata.rectangleCount) * FramePipeLimits.rectangleRecord
    let rectangles = try FrameRectangle.records(
      packet.payload.prefix(records),
      width: metadata.width,
      height: metadata.height,
      payloadLength: UInt64(packet.payload.count)
    )
    let corners = rectangles.map { [Int($0.x), Int($0.y), Int($0.width), Int($0.height)] }
    #expect(corners == (expected["rectangles"] as? [[Int]]), "\(name)")
    if let applied = expected["applied"] as? String {
      let picture = FramePipeFixture.apply(
        full,
        rectangles: rectangles,
        pixels: packet.payload.dropFirst(records),
        width: metadata.width
      )
      #expect(picture == FramePipeFixture.unhex(applied), "\(name)")
    }
  }
}
