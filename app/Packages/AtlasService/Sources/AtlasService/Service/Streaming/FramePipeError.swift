/// Why the frame pipe refused bytes or storage. A refused lane is closed: nothing resumes inside a packet.
public enum FramePipeError: Error, Sendable, Equatable {
  /// Not this protocol: wrong magic, flags, kind, lane, request ID or metadata.
  case invalid
  /// Another protocol major version.
  case unsupportedVersion
  /// A length or count beyond the protocol's bounds.
  case oversized
  /// The lane ended inside a packet.
  case truncated
  /// The storage budget cannot hold what was asked for.
  case budgetExhausted

  /// The diagnostic reason, in the protocol's fixed words.
  public var reason: String {
    switch self {
    case .invalid: "invalid-schema"
    case .unsupportedVersion: "unsupported-version"
    case .oversized: "oversized"
    case .truncated: "truncated"
    case .budgetExhausted: "budget-exhausted"
    }
  }
}
