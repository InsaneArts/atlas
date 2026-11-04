import Foundation

/// What every picture packet says of itself, with exactly its keys and their shapes.
struct FramePictureMetadata: Sendable, Equatable, Decodable {
  /// The broker instance that sent it.
  let serviceInstance: String
  /// The observer it was sent to.
  let observerID: String
  /// The observer's generation.
  let observerGeneration: UInt64
  /// The source stream's generation.
  let streamGeneration: UInt64
  /// The capture geometry's generation.
  let geometryGeneration: UInt64
  /// The credit it serves.
  let creditID: UInt64
  /// The source acquisition it shows.
  let sourceSequence: UInt64
  /// Its image's sequence.
  let imageSequence: UInt64
  /// The image a delta applies to; zero for a full picture.
  let baseImageSequence: UInt64
  /// Its 256-bit random key.
  let frameKey: String
  /// The client's view generation it was made for.
  let viewGeneration: UInt64
  /// The last input revision completed before it.
  let inputRevision: UInt64
  /// Its width, in pixels.
  let width: UInt32
  /// Its height, in pixels.
  let height: UInt32
  /// The bytes of one row: three a pixel.
  let rowStride: UInt32
  /// The helper's display width, in points.
  let displayWidth: Double
  /// The helper's display height, in points.
  let displayHeight: Double
  /// The helper's backing scale.
  let displayScale: Double
  /// Its age when the header was written, in nanoseconds.
  let ageAtHeaderNs: UInt64
  /// The rectangles a delta carries; zero otherwise.
  let rectangleCount: UInt32

  /// The keys whose values are plain decimal integers.
  private static let integers: Set<String> = [
    "observerGeneration", "streamGeneration", "geometryGeneration", "creditID", "sourceSequence", "imageSequence",
    "baseImageSequence", "viewGeneration", "inputRevision", "width", "height", "rowStride", "ageAtHeaderNs",
    "rectangleCount",
  ]
  /// The keys whose values are numbers of any syntax.
  private static let numbers: Set<String> = ["displayWidth", "displayHeight", "displayScale"]
  /// The keys whose values are identifiers.
  private static let identifiers: Set<String> = ["serviceInstance", "observerID", "frameKey"]

  /// The metadata `data` holds, once it has exactly these keys in their shapes.
  ///
  /// - Parameter data: The packet's metadata.
  /// - Throws: ``FramePipeError`` for metadata the validator refuses, a key missing or unknown, or a value outside its
  ///   shape.
  init(validating data: Data) throws(FramePipeError) {
    let found = try FrameJSONValidator.validate(data)
    let kinds = { (key: String) in found[Array(key.utf8)] }
    let keys = Self.integers
      .union(Self.numbers)
      .union(Self.identifiers)
    guard
      Set(found.keys) == Set(keys.map { Array($0.utf8) }),
      Self.integers.allSatisfy({ kinds($0) == .integer }),
      Self.numbers.allSatisfy({ kinds($0) == .integer || kinds($0) == .number }),
      Self.identifiers.allSatisfy({ kinds($0) == .string }),
      let decoded = try? JSONDecoder().decode(Self.self, from: data),
      decoded.isShaped
    else { throw .invalid }
    self = decoded
  }

  /// The full picture's bytes.
  var pictureBytes: UInt64 {
    UInt64(width) * UInt64(height) * 3
  }

  /// Checks that the metadata fits a packet of `kind` with `payloadLength` bytes: a full picture is exactly its pixels,
  /// a delta names 1 to 256 rectangles on a base and is smaller than a full picture, and an unchanged one has no
  /// payload and keeps its image. Checked before any payload is reserved.
  ///
  /// - Parameters:
  ///   - kind: The packet's kind.
  ///   - payloadLength: The packet's payload length.
  /// - Throws: ``FramePipeError/invalid`` for a metadata and length that disagree.
  func check(kind: FramePacketKind, payloadLength: UInt64) throws(FramePipeError) {
    let fits =
      switch kind {
      case .fullFrame:
        baseImageSequence == 0 && rectangleCount == 0 && payloadLength == pictureBytes
      case .deltaFrame:
        (1...FramePipeLimits.rectangles) ~= rectangleCount && baseImageSequence > 0
          && payloadLength > UInt64(FramePipeLimits.rectangleRecord) * UInt64(rectangleCount)
          && payloadLength < pictureBytes
      case .unchangedFrame:
        rectangleCount == 0 && payloadLength == 0 && baseImageSequence == imageSequence
      default: false
      }
    guard fits else { throw .invalid }
  }

  /// Whether every value keeps its shape: identifiers of fixed lowercase hex, positive generations and sequences,
  /// sides within the protocol's bound, an exact stride and finite positive display geometry.
  private var isShaped: Bool {
    let positive = [
      observerGeneration, streamGeneration, geometryGeneration, creditID, sourceSequence, imageSequence,
      viewGeneration,
    ]
    let sides = UInt32(1)...FramePipeLimits.side
    let display = [displayWidth, displayHeight, displayScale]
    return Self.isHex(serviceInstance, digits: 32) && Self.isHex(observerID, digits: 32)
      && Self.isHex(frameKey, digits: 64) && positive.allSatisfy { $0 > 0 } && sides.contains(width)
      && sides.contains(height) && UInt64(rowStride) == UInt64(width) * 3
      && display.allSatisfy { $0.isFinite && $0 > 0 } && rectangleCount <= FramePipeLimits.rectangles
  }

  /// Whether `text` is exactly `digits` lowercase hex digits.
  private static func isHex(_ text: String, digits: Int) -> Bool {
    text.utf8.count == digits && text.utf8.allSatisfy { (0x30...0x39) ~= $0 || (0x61...0x66) ~= $0 }
  }
}
