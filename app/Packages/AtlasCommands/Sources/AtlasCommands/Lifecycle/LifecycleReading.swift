import AtlasService

/// What reading a request came to: the request, or why it can't be shown.
public enum LifecycleReading: Equatable, Sendable {
  /// The request, to show the person.
  case request(LifecycleRequest)
  /// Why it can't be shown: the command line's own sentence, or that its reply could not be read.
  case failed(String)

  /// The words for a reply that worked but holds no request this app can show.
  static let unreadable = "Atlas couldn't read this request."

  /// What `atlas lifecycle request` answered. A request of a kind this app does not know is not shown: the person
  /// could not tell what they would allow.
  ///
  /// - Parameter outcome: The command's outcome.
  /// - Returns: The request, or why it can't be shown.
  public static func of(_ outcome: Outcome) -> LifecycleReading {
    /// The reply's one field the window needs.
    struct Reply: Decodable {
      /// The request.
      let request: LifecycleRequest
    }
    guard outcome.ok else { return .failed(outcome.message) }
    guard let reply = outcome.decode(Reply.self) else { return .failed(unreadable) }
    return .request(reply.request)
  }
}
