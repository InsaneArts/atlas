import Foundation
import Testing

@testable import AtlasService

@Suite("Frame picture metadata")
struct FramePictureMetadataTests {
  /// A full 4 × 2 picture's metadata.
  private static var full: [String: Any] {
    [
      "serviceInstance": String(repeating: "a", count: 32), "observerID": String(repeating: "b", count: 32),
      "observerGeneration": 1, "streamGeneration": 1, "geometryGeneration": 1, "creditID": 1, "sourceSequence": 1,
      "imageSequence": 1, "baseImageSequence": 0, "frameKey": String(repeating: "c", count: 64), "viewGeneration": 1,
      "inputRevision": 0, "width": 4, "height": 2, "rowStride": 12, "displayWidth": 2.0, "displayHeight": 1,
      "displayScale": 2.0, "ageAtHeaderNs": 1_000, "rectangleCount": 0,
    ]
  }

  /// `full` with `changes`, as metadata.
  private static func metadata(_ changes: [String: Any] = [:], removing: String? = nil) throws -> Data {
    var object = full.merging(changes) { $1 }
    if let removing {
      object[removing] = nil
    }
    return try JSONSerialization.data(withJSONObject: object)
  }

  @Test("exactly its keys, each in its shape")
  func exactKeys() throws {
    let picture = try FramePictureMetadata(validating: Self.metadata())
    #expect(picture.pictureBytes == 24)
    #expect(picture.displayHeight == 1)
    #expect(throws: FramePipeError.invalid) { try FramePictureMetadata(validating: Self.metadata(["extra": 1])) }
    #expect(throws: FramePipeError.invalid) {
      try FramePictureMetadata(validating: Self.metadata(removing: "inputRevision"))
    }
    let wrong: [String: Any] = [
      "creditID": 0, "width": 0, "height": 4_097, "rowStride": 13, "displayScale": 0, "observerGeneration": 1.5,
      "serviceInstance": String(repeating: "A", count: 32), "frameKey": "c", "observerID": 7, "rectangleCount": 257,
    ]
    for (key, value) in wrong {
      #expect(throws: FramePipeError.invalid, "\(key)") {
        try FramePictureMetadata(validating: Self.metadata([key: value]))
      }
    }
    let largest = try FramePictureMetadata(validating: Self.metadata(["inputRevision": UInt64.max]))
    #expect(largest.inputRevision == UInt64.max)
    let text = try #require(String(data: Self.metadata(["inputRevision": 7]), encoding: .utf8))
    for (from, to) in [
      (#""inputRevision":7"#, #""inputRevision":18446744073709551616"#),
      (#""frameKey""#, "\"frame\u{212A}ey\""),
    ] {
      #expect(throws: FramePipeError.invalid, "\(to)") {
        try FramePictureMetadata(validating: Data(text.replacingOccurrences(of: from, with: to).utf8))
      }
    }
    let spelled = try #require(String(data: Self.metadata(), encoding: .utf8))
      .replacingOccurrences(of: #""creditID":1"#, with: #""creditID":1.0"#)
    #expect(throws: FramePipeError.invalid, "an integer in plain syntax only") {
      try FramePictureMetadata(validating: Data(spelled.utf8))
    }
  }

  @Test("each picture kind's payload agrees with its metadata")
  func kindsAndLengths() throws {
    let full = try FramePictureMetadata(validating: Self.metadata())
    try full.check(kind: .fullFrame, payloadLength: 24)
    #expect(throws: FramePipeError.invalid) { try full.check(kind: .fullFrame, payloadLength: 23) }
    #expect(throws: FramePipeError.invalid) { try full.check(kind: .credit, payloadLength: 24) }
    for changes: [String: Any] in [["baseImageSequence": 1], ["rectangleCount": 1]] {
      let marked = try FramePictureMetadata(validating: Self.metadata(changes))
      #expect(throws: FramePipeError.invalid, "a full picture names no base or rectangles") {
        try marked.check(kind: .fullFrame, payloadLength: 24)
      }
    }
    let delta = try FramePictureMetadata(
      validating: Self.metadata(["baseImageSequence": 1, "imageSequence": 2, "rectangleCount": 1])
    )
    try delta.check(kind: .deltaFrame, payloadLength: 19)
    #expect(throws: FramePipeError.invalid, "as large as a full picture") {
      try delta.check(kind: .deltaFrame, payloadLength: 24)
    }
    #expect(throws: FramePipeError.invalid, "no pixels past its records") {
      try delta.check(kind: .deltaFrame, payloadLength: 16)
    }
    let unchanged = try FramePictureMetadata(
      validating: Self.metadata(["baseImageSequence": 1, "imageSequence": 1, "sourceSequence": 2])
    )
    try unchanged.check(kind: .unchangedFrame, payloadLength: 0)
    #expect(throws: FramePipeError.invalid) { try full.check(kind: .unchangedFrame, payloadLength: 0) }
  }
}
