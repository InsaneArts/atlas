import Foundation

/// What the service's feed answered a reader.
enum FeedAnswer: Equatable, Sendable {
  /// The events after the reader's position, oldest first, by kind; `last` is the newest one's position, nil when
  /// none came. `poll` says every wait was taken: the reader leaves it a moment and asks again.
  case events(kinds: Set<ChangeKind>, last: FeedPosition?, poll: Bool)
  /// The reader's position does not follow on: it takes this one, then reads everything again.
  case reset(FeedPosition)
  /// The feed cannot be written now.
  case unavailable

  /// The answer `data` holds, as a reply of `status`.
  ///
  /// - Parameters:
  ///   - data: The reply's body.
  ///   - status: The reply's status code.
  /// - Returns: The answer.
  /// - Throws: ``ServiceError`` when it is none of the answers the feed gives.
  static func read(_ data: Data, status: Int) throws(ServiceError) -> FeedAnswer {
    guard status != 503 else { return .unavailable }
    guard status == 200, let body = try? JSONDecoder().decode(Body.self, from: data) else {
      throw ServiceError("Atlas's feed gave no answer.")
    }
    if let reset = body.reset { return .reset(FeedPosition(feed: reset.feed, after: reset.head)) }
    let events = body.events ?? []
    let kinds = events.reduce(into: Set<ChangeKind>()) { kinds, event in
      kinds.formUnion(ChangeKind.named(event.kind ?? ""))
    }
    let last = events.last.map { FeedPosition(feed: $0.feed, after: $0.seq) }
    return .events(kinds: kinds, last: last, poll: body.poll ?? false)
  }

  /// A reply's body, every field the reader does not need left out.
  private struct Body: Decodable {
    /// A position that does not follow on.
    let reset: Reset?
    /// The events after the position.
    let events: [Event]?
    /// Whether every wait was taken.
    let poll: Bool?
  }

  /// The position a reset gives.
  private struct Reset: Decodable {
    /// The feed's identity.
    let feed: String
    /// Its newest event.
    let head: UInt64
  }

  /// One event, as far as a reader needs it.
  private struct Event: Decodable {
    /// The feed it is in.
    let feed: String
    /// Its number.
    let seq: UInt64
    /// What to read again; nil reads as a kind this app does not know.
    let kind: String?
  }
}
