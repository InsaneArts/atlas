import Foundation

/// One lane's packets: a fixed 32-byte prefix, one JSON metadata object, then an optional binary payload. A stream
/// socket may split or join packets anywhere, so decoding keeps its place across reads and checks every length before
/// it reserves anything. Every integer is unsigned big-endian.
///
/// The prefix is `ATFP`, the protocol major (2 bytes), the kind (2), flags of zero (4), the metadata length (4), the
/// payload length (8) and the request ID (8).
public struct FramePacketCodec: Sendable {
  /// The prefix's first four bytes.
  private static let magic: [UInt8] = Array("ATFP".utf8)

  /// The lane decoded.
  public let lane: FramePipeLane
  /// The prefix read so far.
  private var prefix: [UInt8] = []
  /// The header whose metadata is being read.
  private var header: FramePipeHeader?
  /// The metadata read so far.
  private var metadata = Data()
  /// The payload bytes still to come.
  private var left: UInt64 = 0

  /// Creates a decoder for `lane`.
  ///
  /// - Parameter lane: The lane decoded.
  public init(lane: FramePipeLane) {
    self.lane = lane
  }

  /// The next item from the front of `input`, consuming what it uses; nil when `input` ran out first. A refusal ends
  /// the lane.
  ///
  /// - Parameter input: The bytes read and not yet decoded.
  /// - Returns: The item, or nil.
  /// - Throws: ``FramePipeError`` for a prefix that is not this protocol or names lengths past its bounds, before any
  ///   metadata is reserved.
  public mutating func next(_ input: inout Data) throws(FramePipeError) -> FramePacketItem? {
    while true {
      if left > 0 {
        guard !input.isEmpty else { return nil }
        let take = Int(min(left, UInt64(min(FramePipeLimits.chunk, input.count))))
        let piece = Data(input.prefix(take))
        input = input.dropFirst(take)
        left -= UInt64(take)
        return .payload(piece)
      }
      if let header {
        let take = min(header.metadataLength - metadata.count, input.count)
        metadata.append(input.prefix(take))
        input = input.dropFirst(take)
        guard metadata.count == header.metadataLength else { return nil }
        let complete = metadata
        metadata = Data()
        self.header = nil
        left = header.payloadLength
        return .packet(header, complete)
      }
      let take = min(FramePipeLimits.prefix - prefix.count, input.count)
      prefix.append(contentsOf: input.prefix(take))
      input = input.dropFirst(take)
      guard prefix.count == FramePipeLimits.prefix else { return nil }
      let header = try Self.header(prefix, lane: lane)
      prefix = []
      metadata.reserveCapacity(header.metadataLength)
      self.header = header
    }
  }

  /// Checks that the lane may end here: between packets, not inside one.
  ///
  /// - Throws: ``FramePipeError/truncated`` for an end inside a packet.
  public func end() throws(FramePipeError) {
    guard prefix.isEmpty, header == nil, left == 0 else { throw .truncated }
  }

  /// The prefix and metadata of one packet whose payload is `payloadLength` bytes, checked against the protocol's
  /// bounds before anything is written.
  ///
  /// - Parameters:
  ///   - kind: The packet's kind.
  ///   - request: The request it makes or answers.
  ///   - metadata: Its metadata object.
  ///   - payloadLength: The length of the payload that follows, in bytes.
  /// - Returns: The bytes to write before the payload.
  /// - Throws: ``FramePipeError`` for a kind with a payload it may not carry, metadata or a payload past their bounds,
  ///   or request ID zero for a kind that answers requests.
  public static func encode(
    kind: FramePacketKind,
    request: UInt64,
    metadata: Data,
    payloadLength: UInt64
  ) throws(FramePipeError) -> Data {
    let header = FramePipeHeader(
      kind: kind,
      metadataLength: metadata.count,
      payloadLength: payloadLength,
      request: request
    )
    try header.check()
    var bytes = Data(magic)
    bytes.append(contentsOf: bigEndian(FramePipeLimits.major))
    bytes.append(contentsOf: bigEndian(kind.rawValue))
    bytes.append(contentsOf: bigEndian(UInt32(0)))
    bytes.append(contentsOf: bigEndian(UInt32(metadata.count)))
    bytes.append(contentsOf: bigEndian(payloadLength))
    bytes.append(contentsOf: bigEndian(request))
    bytes.append(metadata)
    return bytes
  }

  /// The header a complete prefix names on `lane`.
  private static func header(_ bytes: [UInt8], lane: FramePipeLane) throws(FramePipeError) -> FramePipeHeader {
    guard Array(bytes[0..<4]) == magic else { throw .invalid }
    guard UInt16(word(bytes, at: 4, width: 2)) == FramePipeLimits.major else { throw .unsupportedVersion }
    guard
      let kind = FramePacketKind(rawValue: UInt16(word(bytes, at: 6, width: 2))),
      kind.lanes.contains(lane)
    else { throw .invalid }
    guard word(bytes, at: 8, width: 4) == 0 else { throw .invalid }
    let header = FramePipeHeader(
      kind: kind,
      metadataLength: Int(word(bytes, at: 12, width: 4)),
      payloadLength: word(bytes, at: 16, width: 8),
      request: word(bytes, at: 24, width: 8)
    )
    try header.check()
    return header
  }

  /// The big-endian unsigned integer of `width` bytes at `at`.
  private static func word(_ bytes: [UInt8], at: Int, width: Int) -> UInt64 {
    bytes[at..<(at + width)].reduce(0) { $0 << 8 | UInt64($1) }
  }

  /// `value`'s bytes, most significant first.
  private static func bigEndian<T: FixedWidthInteger>(_ value: T) -> [UInt8] {
    (0..<(T.bitWidth / 8)).reversed().map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) }
  }
}
