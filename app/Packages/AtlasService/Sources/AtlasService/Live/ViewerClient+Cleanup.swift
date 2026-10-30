import Foundation

extension ViewerClient {
  /// How long a cleanup may take, in seconds: 60, well beyond quitting the session's apps (up to 10 s), then rolling
  /// back its plans and moving what it installed aside in the helper and removing what it copied.
  static let cleanupTimeout: TimeInterval = 60
  /// The code of an apply's reply that ran and did not finish: it still lists its steps.
  static let unfinishedCleanupCode = "cleanup"
  /// The code of an apply that got no reply: it timed out or failed on the way, so the cleanup may still run.
  public static let unansweredCleanupCode = "cleanup-unanswered"

  /// Reads `cleanup.json` for the session, or posts to `cleanup` to clean up its work as the person.
  ///
  /// - Parameters:
  ///   - session: The session's key.
  ///   - apply: Whether to clean up now, rather than read what it would take.
  /// - Returns: The steps it takes, or took and how far each got, every missing field at its safe default; an apply
  ///   that did not finish comes back too, with `ok` false.
  /// - Throws: ``ServiceError/notOffered`` when an older service answers 404; for an apply that got no reply, a
  ///   ``ServiceError`` with ``unansweredCleanupCode``, as it may still run; else ``ServiceError`` with the service's
  ///   error, such as for a key it does not know or while a viewer holds control, or when it cannot be read.
  public func cleanup(session: String, apply: Bool) async throws(ServiceError) -> CleanupReply {
    let request = try apply ? cleanupApplyRequest(session: session) : cleanupPreviewRequest(session: session)
    let data: Data
    let response: HTTPURLResponse
    do {
      (data, response) = try await fetched(request, limit: Self.listLimit)
    } catch {
      guard apply, !error.isCancelled else { throw error }
      throw ServiceError(error.message, code: Self.unansweredCleanupCode)
    }
    return try await Self.decodedReply { () throws(ServiceError) in try Self.cleanup(data, response: response) }
  }

  /// A GET of `cleanup.json` for `session`, the key percent-encoded.
  ///
  /// - Parameter session: The session's key.
  /// - Returns: The request.
  /// - Throws: ``ServiceError`` when the key cannot be put in a URL.
  func cleanupPreviewRequest(session: String) throws(ServiceError) -> URLRequest {
    let route = base.appendingPathComponent("cleanup.json")
    guard let encoded = session.addingPercentEncoding(withAllowedCharacters: Self.unreserved),
      var components = URLComponents(url: route, resolvingAgainstBaseURL: false)
    else { throw ServiceError("No such session") }
    components.percentEncodedQuery = "session=\(encoded)"
    guard let url = components.url else { throw ServiceError("No such session") }
    return request(to: url)
  }

  /// A POST of `session` to `cleanup`, given ``cleanupTimeout`` to answer.
  ///
  /// - Parameter session: The session's key.
  /// - Returns: The request.
  /// - Throws: ``ServiceError`` when the body cannot be written, which a key never makes happen.
  func cleanupApplyRequest(session: String) throws(ServiceError) -> URLRequest {
    /// The request: the session whose work to clean up.
    struct Body: Encodable {
      /// The session's key.
      let session: String
    }
    do {
      return try request("cleanup", body: Body(session: session), timeout: Self.cleanupTimeout)
    } catch {
      throw ServiceError("No such session")
    }
  }

  /// A cleanup's reply read: a 200 carries the preview or a finished apply, and a 409 that ran and did not finish
  /// still carries its steps, so it reads as a reply; a 404 says the service does not offer cleanup.
  ///
  /// - Parameters:
  ///   - data: The reply's body.
  ///   - response: The reply's response.
  /// - Returns: The reply, every missing field at its safe default.
  /// - Throws: ``ServiceError/notOffered`` for a 404; ``ServiceError`` with the service's error for any other refusal,
  ///   or when its body is not an object.
  static func cleanup(_ data: Data, response: HTTPURLResponse) throws(ServiceError) -> CleanupReply {
    guard response.statusCode != 404 else { throw .notOffered }
    let refusal = failure(data, fallback: "Cleanup was refused")
    guard response.statusCode == 200 || (response.statusCode == 409 && refusal.code == unfinishedCleanupCode) else {
      throw refusal
    }
    guard let reply = decoded(CleanupReply.self, from: data) else { throw ServiceError("No cleanup") }
    return reply
  }
}
