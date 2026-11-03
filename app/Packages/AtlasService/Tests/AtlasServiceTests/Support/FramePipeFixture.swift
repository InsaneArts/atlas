import Foundation
import Testing

@testable import AtlasService

/// The shared golden vectors, which the Rust pipe decodes too.
enum FramePipeFixture {
  /// The vectors' folder.
  static func folder() throws -> URL {
    try #require(Bundle.module.url(forResource: "FramePipe", withExtension: nil)).appending(path: "v1")
  }

  /// The manifest: one entry a vector.
  static func manifest() throws -> [[String: Any]] {
    let data = try Data(contentsOf: folder().appending(path: "manifest.json"))
    return try #require(try JSONSerialization.jsonObject(with: data) as? [[String: Any]])
  }

  /// The bytes `text` spells in hex.
  static func unhex(_ text: String) -> Data {
    let digits = Array(text.utf8)
    return Data(
      stride(from: 0, to: digits.count, by: 2)
        .compactMap { UInt8(String(decoding: digits[$0..<($0 + 2)], as: UTF8.self), radix: 16) }
    )
  }

  /// Every packet in `bytes` on `lane`, fed `step` bytes at a time.
  static func packets(lane: FramePipeLane, bytes: Data, step: Int) throws(FramePipeError) -> [DecodedFramePacket] {
    var codec = FramePacketCodec(lane: lane)
    var found: [DecodedFramePacket] = []
    for start in stride(from: 0, to: bytes.count, by: step) {
      var input = bytes.subdata(in: start..<min(start + step, bytes.count))
      while let item = try codec.next(&input) {
        switch item {
        case .packet(let header, let metadata):
          found.append(DecodedFramePacket(header: header, metadata: metadata))
        case .payload(let piece):
          found[found.count - 1].payload.append(piece)
        }
      }
    }
    try codec.end()
    return found
  }

  /// `base` with each rectangle's pixels from `pixels` written in, row by row.
  static func apply(_ base: Data, rectangles: [FrameRectangle], pixels: Data, width: UInt32) -> Data {
    var picture = Array(base)
    let source = Array(pixels)
    var at = 0
    for rectangle in rectangles {
      let row = Int(rectangle.width) * 3
      for line in 0..<rectangle.height {
        let start = Int((rectangle.y + line) * width + rectangle.x) * 3
        picture.replaceSubrange(start..<(start + row), with: source[at..<(at + row)])
        at += row
      }
    }
    return Data(picture)
  }
}
