import Foundation

/// The service's feed of what changed: one wait on `events.json`.
extension ViewerClient {
  /// How long one wait on the feed may take, in seconds: longer than the service waits.
  static let feedTimeout: TimeInterval = 35
  /// The most one answer of the feed holds, in bytes: a batch of events.
  static let feedLimit = 1 << 17

  /// Asks the feed what changed after `position`, waiting a while for something; with none, where it stands.
  ///
  /// - Parameter position: Where the reader stands; nil to be told.
  /// - Returns: The feed's answer.
  /// - Throws: ``ServiceError`` when it could not be asked or gave no answer.
  func events(after position: FeedPosition?) async throws(ServiceError) -> FeedAnswer {
    var route = URLComponents(url: base.appendingPathComponent("events.json"), resolvingAgainstBaseURL: false)
    if let position {
      route?.queryItems = [
        URLQueryItem(name: "feed", value: position.feed),
        URLQueryItem(name: "after", value: String(position.after)),
      ]
    }
    guard let url = route?.url else { throw ServiceError("Atlas's feed gave no answer.") }
    let (data, response) = try await fetched(request(to: url, timeout: Self.feedTimeout), limit: Self.feedLimit)
    return try FeedAnswer.read(data, status: response.statusCode)
  }
}
