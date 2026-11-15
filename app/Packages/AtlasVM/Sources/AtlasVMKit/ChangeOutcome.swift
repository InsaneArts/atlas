/// What a change came to, once it can be told. One that cannot be told is
/// never given an outcome: it stays pending.
public enum ChangeOutcome: Equatable, Sendable {
  /// It was done, and seen done.
  case done
  /// It was not done, or is not in effect, for this reason.
  case refused(String)
}

extension ChangeOutcome: Codable {
  private enum Key: String, CodingKey {
    case state
    case reason
  }

  /// Read as it is written.
  public init(from decoder: any Decoder) throws {
    let fields = try decoder.container(keyedBy: Key.self)
    switch try fields.decode(String.self, forKey: .state) {
    case "done":
      self = .done
    case "refused":
      self = .refused(try fields.decode(String.self, forKey: .reason))
    default:
      throw DecodingError.dataCorruptedError(
        forKey: .state,
        in: fields,
        debugDescription: "not an outcome"
      )
    }
  }

  /// Written as its state, with a refusal's reason.
  public func encode(to encoder: any Encoder) throws {
    var fields = encoder.container(keyedBy: Key.self)
    switch self {
    case .done:
      try fields.encode("done", forKey: .state)
    case .refused(let reason):
      try fields.encode("refused", forKey: .state)
      try fields.encode(reason, forKey: .reason)
    }
  }
}
