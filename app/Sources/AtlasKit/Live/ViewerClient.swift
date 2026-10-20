import Foundation

/// The local viewer service, reached only through its capability URL (which
/// arrives on standard input, never in arguments). Requests carry the
/// service's own origin, as it requires.
package struct ViewerClient: ViewerService {
  /// How long a request may take, in seconds: 8, well beyond a picture's capture, short enough that a silent helper
  /// reads as unavailable soon.
  static let timeout: TimeInterval = 8
  /// How long recovery may take, in seconds: 20, since it may restart the helper's input service.
  static let recoveryTimeout: TimeInterval = 20

  /// The capability URL every route is under.
  let base: URL
  /// The service's own origin, which every request carries.
  let origin: String
  /// An ephemeral session that caches nothing.
  let session: URLSession

  /// A client for `text`, which must be the service's local capability URL.
  ///
  /// - Parameter text: The capability URL: `http://127.0.0.1:<port>/<secret>/`, surrounding whitespace allowed.
  package init?(_ text: String) {
    guard let url = URL(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
      url.scheme == "http", url.host == "127.0.0.1", let port = url.port,
      url.absoluteString.hasSuffix("/")
    else { return nil }
    base = url
    origin = "http://127.0.0.1:\(port)"
    let configuration = URLSessionConfiguration.ephemeral
    configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
    configuration.timeoutIntervalForRequest = Self.timeout
    session = URLSession(configuration: configuration)
  }

  /// A request for `route`: a POST of `body` as JSON when there is one, else a GET.
  func request(
    _ route: String,
    body: (any Encodable & Sendable)? = nil,
    timeout: TimeInterval = ViewerClient.timeout
  ) throws -> URLRequest {
    var request = URLRequest(url: base.appendingPathComponent(route))
    request.timeoutInterval = timeout
    request.setValue(origin, forHTTPHeaderField: "Origin")
    if let body {
      request.httpMethod = "POST"
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      let encoder = JSONEncoder()
      request.httpBody = try encoder.encode(body)
    }
    return request
  }

  /// Reads `status.json`.
  ///
  /// - Returns: The status, every missing field at its safe default.
  /// - Throws: A transport error, or `DecodingError` when the reply is not an object.
  package func status() async throws -> ViewerStatus {
    let (data, _) = try await session.data(for: request("status.json"))
    let decoder = JSONDecoder()
    return try decoder.decode(ViewerStatus.self, from: data)
  }

  /// Reads `frame.png`, with what it was taken of from its headers.
  ///
  /// - Returns: The picture, its id, revision and display size.
  /// - Throws: ``ServiceError`` when the capture failed or a header is missing, or a transport error.
  package func frame() async throws -> Frame {
    let (data, response) = try await session.data(for: request("frame.png"))
    guard let http = response as? HTTPURLResponse else { throw ServiceError("No picture") }
    guard http.statusCode == 200 else { throw Self.failure(data, fallback: "Capture failed") }
    let header = { (name: String) in http.value(forHTTPHeaderField: name) }
    guard let id = header("X-Frame-Id"), !id.isEmpty,
      let revision = header("X-Session-Revision"), !revision.isEmpty,
      let width = header("X-Display-Width").flatMap(Double.init), width > 0,
      let height = header("X-Display-Height").flatMap(Double.init), height > 0
    else { throw ServiceError.unknownScreenSize }
    return Frame(png: data, id: id, revision: revision, display: .init(width: width, height: height))
  }

  /// Posts to `control/acquire`.
  ///
  /// - Parameter frame: The id of the picture the person acts on.
  /// - Returns: The lease the service granted.
  /// - Throws: ``ServiceError`` when control is not granted.
  package func acquire(frame: String) async throws -> String {
    struct Body: Encodable { let frame: String }
    struct Reply: Decodable { let lease: String }
    let data = try await post("control/acquire", Body(frame: frame))
    guard let reply = Self.decoded(Reply.self, from: data) else {
      throw ServiceError("Control was not granted")
    }
    return reply.lease
  }

  /// Posts to `control/renew`.
  ///
  /// - Parameters:
  ///   - lease: The lease to renew.
  ///   - frame: The id of the picture now shown.
  /// - Throws: ``ServiceError`` when the service refuses.
  package func renew(lease: String, frame: String) async throws {
    struct Body: Encodable {
      let lease: String
      let frame: String
    }
    _ = try await post("control/renew", Body(lease: lease, frame: frame))
  }

  /// Posts to `control/release`.
  ///
  /// - Parameter lease: The lease to end.
  /// - Throws: ``ServiceError`` when the service refuses.
  package func release(lease: String) async throws {
    struct Body: Encodable { let lease: String }
    _ = try await post("control/release", Body(lease: lease))
  }

  /// Posts to `control/revoke`.
  ///
  /// - Throws: ``ServiceError`` when the service refuses.
  package func revoke() async throws {
    struct Body: Encodable {}
    _ = try await post("control/revoke", Body())
  }

  /// Posts to `input`.
  ///
  /// - Parameters:
  ///   - lease: The lease the input belongs to.
  ///   - frame: The id of the picture the input's coordinates refer to.
  ///   - actions: The actions, in order.
  /// - Throws: ``ServiceError`` when the service refuses.
  package func input(lease: String, frame: String, actions: [InputAction]) async throws {
    struct Body: Encodable {
      let lease: String
      let frame: String
      let actions: [InputAction]
    }
    _ = try await post("input", Body(lease: lease, frame: frame, actions: actions))
  }

  /// Recover the helper; whether it restarted (rather than starting input).
  ///
  /// - Returns: Whether the helper restarted.
  /// - Throws: ``ServiceError`` when recovery did not run.
  package func recover() async throws -> Bool {
    struct Body: Encodable {}
    struct Reply: Decodable { let restarted: Bool? }
    let data = try await post("recover", Body(), timeout: Self.recoveryTimeout)
    return Self.decoded(Reply.self, from: data)?.restarted == true
  }

  /// POST `body` to `route`; the reply when it says `ok`, else its error.
  func post(
    _ route: String,
    _ body: some Encodable & Sendable,
    timeout: TimeInterval = ViewerClient.timeout
  ) async throws -> Data {
    let data: Data
    let response: URLResponse
    do {
      (data, response) = try await session.data(for: request(route, body: body, timeout: timeout))
    } catch let error as URLError where error.code == .timedOut {
      throw ServiceError("The helper did not answer in time.")
    }
    let ok = Self.decoded(Acknowledgement.self, from: data)?.ok == true
    guard (response as? HTTPURLResponse)?.statusCode == 200, ok else {
      throw Self.failure(data, fallback: "Request was refused")
    }
    return data
  }

  /// Every POST reply says whether it worked.
  private struct Acknowledgement: Decodable { let ok: Bool? }

  /// The service's own error in `data`, else `fallback`.
  static func failure(_ data: Data, fallback: String) -> ServiceError {
    struct Reply: Decodable { let error: String? }
    let reply = decoded(Reply.self, from: data)
    return ServiceError(reply?.error ?? fallback)
  }

  /// A reply decoded as `type`, or nil when it is not one.
  ///
  /// - Parameters:
  ///   - type: The reply's expected shape.
  ///   - data: The reply's body.
  /// - Returns: The decoded reply, or nil.
  private static func decoded<T: Decodable>(_ type: T.Type, from data: Data) -> T? {
    let decoder = JSONDecoder()
    return try? decoder.decode(type, from: data)
  }
}
