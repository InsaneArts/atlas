import AtlasService

/// What answering a request came to: allowed, refused, or not taken.
public enum LifecycleAnswer: Equatable, Sendable {
  /// The command line kept the person's Allow.
  case allowed
  /// The command line kept the person's refusal.
  case refused
  /// The answer was not taken, as the command line says why: the request expired, or was answered already.
  case failed(String)

  /// The words for a reply that worked but says neither.
  static let unreadable = "Atlas couldn't tell whether your answer was kept."

  /// What `atlas lifecycle answer` answered.
  ///
  /// - Parameter outcome: The command's outcome.
  /// - Returns: Allowed, refused, or why the answer was not taken.
  public static func of(_ outcome: Outcome) -> LifecycleAnswer {
    /// The reply's one field.
    struct Reply: Decodable {
      /// `allowed` or `refused`.
      let answer: String
    }
    guard outcome.ok else { return .failed(outcome.message) }
    switch outcome.decode(Reply.self)?.answer {
    case "allowed": return .allowed
    case "refused": return .refused
    default: return .failed(unreadable)
    }
  }
}
