import Foundation

/// What every picture packet says of itself, with exactly its keys and their shapes.
public struct FramePictureMetadata: Sendable, Equatable, Decodable {
  /// The broker instance that sent it.
  public var serviceInstance: String
  /// The observer it was sent to.
  public var observerID: String
  /// The observer's generation.
  public var observerGeneration: UInt64
  /// The source stream's generation.
  public var streamGeneration: UInt64
  /// The capture geometry's generation.
  public var geometryGeneration: UInt64
  /// The credit it serves.
  public var creditID: UInt64
  /// The source acquisition it shows.
  public var sourceSequence: UInt64
  /// Its image's sequence.
  public var imageSequence: UInt64
  /// The image a delta applies to; zero for a full picture.
  public var baseImageSequence: UInt64
  /// Its 256-bit random key.
  public var frameKey: String
  /// The client's view generation it was made for.
  public var viewGeneration: UInt64
  /// The last input revision completed before it.
  public var inputRevision: UInt64
  /// Its width, in pixels.
  public var width: UInt32
  /// Its height, in pixels.
  public var height: UInt32
  /// The bytes of one row: three a pixel.
  public var rowStride: UInt32
  /// The helper's display width, in points.
  public var displayWidth: Double
  /// The helper's display height, in points.
  public var displayHeight: Double
  /// The helper's backing scale.
  public var displayScale: Double
  /// Its age when the header was written, in nanoseconds.
  public var ageAtHeaderNs: UInt64
  /// The rectangles a delta carries; zero otherwise.
  public var rectangleCount: UInt32

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
  public init(validating data: Data) throws(FramePipeError) {
    let kinds = try FrameJSONValidator.validate(data)
    guard
      Set(kinds.keys) == Self.integers.union(Self.numbers).union(Self.identifiers),
      Self.integers.allSatisfy({ kinds[$0] == .integer }),
      Self.numbers.allSatisfy({ kinds[$0] == .integer || kinds[$0] == .number }),
      Self.identifiers.allSatisfy({ kinds[$0] == .string }),
      let decoded = try? JSONDecoder().decode(Self.self, from: data),
      decoded.isShaped
    else { throw .invalid }
    self = decoded
  }

  /// The full picture's bytes.
  public var pictureBytes: UInt64 {
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
  public func check(kind: FramePacketKind, payloadLength: UInt64) throws(FramePipeError) {
    let fits =
      switch kind {
      case .fullFrame:
        baseImageSequence == 0 && rectangleCount == 0 && payloadLength == pictureBytes
      case .deltaFrame:
        (1...FramePipeLimits.rectangles).contains(rectangleCount) && baseImageSequence > 0
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
    text.utf8.count == digits && text.utf8.allSatisfy { (0x30...0x39).contains($0) || (0x61...0x66).contains($0) }
  }
}
