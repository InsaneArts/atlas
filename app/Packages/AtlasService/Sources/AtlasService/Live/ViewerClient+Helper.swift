import Foundation

extension ViewerClient {
  /// The most a list reply may hold, in bytes: 4 MiB. Two hundred activity rows or screenshots take tens of
  /// kilobytes; the apps list carries at most 64 icons of 64 points as base64 PNG, under 2 MiB.
  static let listLimit = 4 << 20
  /// The most a screenshot may hold, in bytes: 16 MiB, the service's own limit on a picture.
  static let pictureLimit = 16 << 20
  /// What a reply over its limit says.
  static let tooLarge = "The helper's reply was too large."
  /// What a request cancelled before its reply says.
  static let cancelledRequest = "The request was cancelled."
  /// The code a cancelled request carries, so a caller tells it apart from the helper's own refusal.
  static let cancelledCode = "cancelled"
  /// The bytes a screenshot's name keeps as they are in its query: RFC 3986's unreserved ones; every other byte is
  /// percent-encoded.
  static let unreserved = CharacterSet(
    charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~"
  )

  /// Reads `activity.json`.
  ///
  /// - Returns: The log, newest first, every missing field at its safe default.
  /// - Throws: ``ServiceError`` with the service's error, or when it cannot be read.
  public func activity() async throws(ServiceError) -> ActivityLog {
    let route = base.appendingPathComponent("activity.json")
    let (data, response) = try await fetched(request(to: route), limit: Self.listLimit)
    return try await Self.decodedReply { () throws(ServiceError) in try Self.activity(data, response: response) }
  }

  /// Reads `apps.json`.
  ///
  /// - Returns: The apps, front to back, and the sessions that opened them, every missing field at its safe default.
  /// - Throws: ``ServiceError`` with the service's error, such as a 503 when it cannot reach the helper, or when it
  ///   cannot be read.
  public func apps() async throws(ServiceError) -> HelperAppList {
    let route = base.appendingPathComponent("apps.json")
    let (data, response) = try await fetched(request(to: route), limit: Self.listLimit)
    return try await Self.decodedReply { () throws(ServiceError) in try Self.apps(data, response: response) }
  }

  /// Reads `screenshots.json`.
  ///
  /// - Returns: The screenshots, newest first, and their folder, every missing field at its safe default.
  /// - Throws: ``ServiceError`` with the service's error, or when it cannot be read.
  public func screenshots() async throws(ServiceError) -> ScreenshotList {
    let route = base.appendingPathComponent("screenshots.json")
    let (data, response) = try await fetched(request(to: route), limit: Self.listLimit)
    return try await Self.decodedReply { () throws(ServiceError) in try Self.screenshots(data, response: response) }
  }

  /// Reads `sessions.json`.
  ///
  /// - Returns: The sessions, newest last seen first, every missing field at its safe default.
  /// - Throws: ``ServiceError/notOffered`` when an older service answers 404; else ``ServiceError`` with the service's
  ///   error, or when it cannot be read.
  public func sessions() async throws(ServiceError) -> HostSessionList {
    let route = base.appendingPathComponent("sessions.json")
    let (data, response) = try await fetched(request(to: route), limit: Self.listLimit)
    return try await Self.decodedReply { () throws(ServiceError) in try Self.sessions(data, response: response) }
  }

  /// Decodes a reply off the caller's actor.
  ///
  /// - Parameter decode: The reply's decoding work.
  /// - Returns: The decoded reply.
  /// - Throws: The decoding error, unchanged.
  @concurrent static func decodedReply<Reply: Sendable, Failure: Error>(
    _ decode: @Sendable () throws(Failure) -> Reply
  ) async throws(Failure) -> Reply {
    try decode()
  }

  /// Reads `screenshot.png`.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - full: Whether to read the picture as taken (PNG), rather than its thumbnail (JPEG).
  /// - Returns: The picture's bytes.
  /// - Throws: ``ServiceError`` with the service's error, such as when no screenshot has that name, or when it
  ///   cannot be read.
  public func screenshot(name: String, full: Bool) async throws(ServiceError) -> Data {
    let (data, response) = try await fetched(screenshotRequest(name: name, full: full), limit: Self.pictureLimit)
    guard response.statusCode == 200 else { throw Self.failure(data, fallback: "No screenshot") }
    return data
  }

  /// A GET of `screenshot.png` for `name` at the size asked for, the name percent-encoded.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - full: Whether to ask for the picture as taken, rather than its thumbnail.
  /// - Returns: The request.
  /// - Throws: ``ServiceError`` when the name cannot be put in a URL.
  func screenshotRequest(name: String, full: Bool) throws(ServiceError) -> URLRequest {
    let route = base.appendingPathComponent("screenshot.png")
    guard let encoded = name.addingPercentEncoding(withAllowedCharacters: Self.unreserved),
      var components = URLComponents(url: route, resolvingAgainstBaseURL: false)
    else { throw ServiceError("No screenshot") }
    components.percentEncodedQuery = "name=\(encoded)&size=\(full ? "full" : "thumb")"
    guard let url = components.url else { throw ServiceError("No screenshot") }
    return request(to: url)
  }

  /// An `activity.json` reply read: only a 200 carries a log.
  ///
  /// - Parameters:
  ///   - data: The reply's body.
  ///   - response: The reply's response.
  /// - Returns: The log, every missing field at its safe default.
  /// - Throws: ``ServiceError`` with the service's error for any reply that is not a 200, or when its body is not an
  ///   object.
  static func activity(_ data: Data, response: HTTPURLResponse) throws(ServiceError) -> ActivityLog {
    try listed(ActivityLog.self, data, response: response, fallback: "No activity")
  }

  /// An `apps.json` reply read: only a 200 carries apps, so a refusal never reads as no apps.
  ///
  /// - Parameters:
  ///   - data: The reply's body.
  ///   - response: The reply's response.
  /// - Returns: The apps, front to back, and the sessions that opened them, every missing field at its safe default.
  /// - Throws: ``ServiceError`` with the service's error for any reply that is not a 200, or when its body is not an
  ///   object.
  static func apps(_ data: Data, response: HTTPURLResponse) throws(ServiceError) -> HelperAppList {
    try listed(HelperAppList.self, data, response: response, fallback: "No apps")
  }

  /// A `screenshots.json` reply read: only a 200 carries screenshots.
  ///
  /// - Parameters:
  ///   - data: The reply's body.
  ///   - response: The reply's response.
  /// - Returns: The screenshots, newest first, and their folder, every missing field at its safe default.
  /// - Throws: ``ServiceError`` with the service's error for any reply that is not a 200, or when its body is not an
  ///   object.
  static func screenshots(_ data: Data, response: HTTPURLResponse) throws(ServiceError) -> ScreenshotList {
    try listed(ScreenshotList.self, data, response: response, fallback: "No screenshots")
  }

  /// A `sessions.json` reply read: only a 200 carries sessions, and a 404 says the service does not offer them.
  ///
  /// - Parameters:
  ///   - data: The reply's body.
  ///   - response: The reply's response.
  /// - Returns: The sessions, every missing field at its safe default.
  /// - Throws: ``ServiceError/notOffered`` for a 404; ``ServiceError`` with the service's error for any other reply
  ///   that is not a 200, or when its body is not an object.
  static func sessions(_ data: Data, response: HTTPURLResponse) throws(ServiceError) -> HostSessionList {
    guard response.statusCode != 404 else { throw .notOffered }
    return try listed(HostSessionList.self, data, response: response, fallback: "No sessions")
  }

  /// A JSON reply decoded as `type` when the service answered with a 200, else the service's error.
  ///
  /// - Parameters:
  ///   - type: The reply's shape.
  ///   - data: The reply's body.
  ///   - response: The reply's response.
  ///   - fallback: What to say when the service gave no error of its own, or the body is not the shape.
  /// - Returns: The decoded reply.
  /// - Throws: ``ServiceError`` with the service's error, or `fallback`.
  static func listed<T: Decodable>(
    _ type: T.Type,
    _ data: Data,
    response: HTTPURLResponse,
    fallback: String
  ) throws(ServiceError) -> T {
    guard response.statusCode == 200 else { throw failure(data, fallback: fallback) }
    guard let reply = decoded(type, from: data) else { throw ServiceError(fallback) }
    return reply
  }

  /// The reply to `request`, refused when it holds more than `limit` bytes.
  ///
  /// The whole reply is read before its size is checked, since URLSession's byte stream is far too slow for a
  /// picture; the limit keeps an oversized reply from ever being decoded or shown.
  ///
  /// - Parameters:
  ///   - request: The request.
  ///   - limit: The most the reply may hold, in bytes.
  /// - Returns: The reply's body and response.
  /// - Throws: ``ServiceError`` when no reply came in time, the request was cancelled (code `cancelled`), the reply is
  ///   too large, or the request failed.
  func fetched(_ request: URLRequest, limit: Int) async throws(ServiceError) -> (Data, HTTPURLResponse) {
    let data: Data
    let response: URLResponse
    do {
      (data, response) = try await session.data(for: request)
    } catch let error as URLError where error.code == .timedOut {
      throw ServiceError(Self.lateReply)
    } catch let error as URLError where error.code == .cancelled {
      throw ServiceError(Self.cancelledRequest, code: Self.cancelledCode)
    } catch is CancellationError {
      throw ServiceError(Self.cancelledRequest, code: Self.cancelledCode)
    } catch {
      throw ServiceError(error.localizedDescription)
    }
    guard let http = response as? HTTPURLResponse else { throw ServiceError("No reply") }
    guard data.count <= limit else { throw ServiceError(Self.tooLarge) }
    return (data, http)
  }
}
