/// A packet's decoded prefix: its kind, the lengths that follow it and the request it belongs to.
public struct FramePipeHeader: Sendable, Equatable {
  /// The packet's kind.
  public var kind: FramePacketKind
  /// The metadata object's length, in bytes.
  public var metadataLength: Int
  /// The payload's length, in bytes; zero for a kind without one.
  public var payloadLength: UInt64
  /// The request the packet makes or answers; zero only for an unsolicited state or error.
  public var request: UInt64

  /// Creates a header.
  ///
  /// - Parameters:
  ///   - kind: The packet's kind.
  ///   - metadataLength: The metadata object's length, in bytes.
  ///   - payloadLength: The payload's length, in bytes.
  ///   - request: The request the packet makes or answers.
  public init(kind: FramePacketKind, metadataLength: Int, payloadLength: UInt64, request: UInt64) {
    self.kind = kind
    self.metadataLength = metadataLength
    self.payloadLength = payloadLength
    self.request = request
  }

  /// Checks the bounds every header keeps, whichever side made it.
  ///
  /// - Throws: ``FramePipeError/oversized`` for a length past its bound, else ``FramePipeError/invalid`` for a
  ///   payload the kind may not carry or a request ID it may not have.
  func check() throws(FramePipeError) {
    // The shortest object, `{}`, has two bytes.
    guard metadataLength >= 2 else { throw .invalid }
    guard metadataLength <= FramePipeLimits.metadata else { throw .oversized }
    if kind.hasPayload {
      guard payloadLength > 0 else { throw .invalid }
      guard payloadLength <= FramePipeLimits.payload else { throw .oversized }
    } else {
      guard payloadLength == 0 else { throw .invalid }
      guard FramePipeLimits.prefix + metadataLength <= FramePipeLimits.control else { throw .oversized }
    }
    guard request != 0 || kind.isUnsolicited else { throw .invalid }
    // A state change is never an answer.
    guard kind != .state || request == 0 else { throw .invalid }
  }
}
