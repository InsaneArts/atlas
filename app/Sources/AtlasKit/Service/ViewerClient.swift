import Foundation

/// Why a request to the viewer service did not work, in the service's own
/// words when it gave any.
public struct ServiceError: Error, Equatable, Sendable, LocalizedError {
    public var message: String
    /// The service's error code (`busy`, `lease-lost`, …), if it gave one.
    public var code: String?
    /// The action may or may not have happened.
    public var uncertain = false

    public init(_ message: String, code: String? = nil, uncertain: Bool = false) {
        self.message = message
        self.code = code
        self.uncertain = uncertain
    }

    public var errorDescription: String? { message }
}

/// One picture of the helper's desktop, with what it was taken of.
public struct Frame: Sendable, Equatable {
    public var png: Data
    /// Identifies this picture to the service (control and input name it).
    public var id: String
    /// Changes whenever the helper's session or display changes.
    public var revision: String
    /// The display, in points.
    public var display: ViewerStatus.Size

    public init(png: Data, id: String, revision: String, display: ViewerStatus.Size) {
        self.png = png
        self.id = id
        self.revision = revision
        self.display = display
    }
}

/// The viewer service, as the app sees it; a protocol so the models are
/// tested with a stand-in.
public protocol ViewerService: Sendable {
    var base: URL { get }
    func status() async throws -> ViewerStatus
    func frame() async throws -> Frame
    func acquire(frame: String) async throws -> String
    func renew(lease: String, frame: String) async throws
    func release(lease: String) async throws
    func revoke() async throws
    func input(lease: String, frame: String, actions: [InputAction]) async throws
    func recover() async throws -> Bool
}

/// The local viewer service, reached only through its capability URL (which
/// arrives on standard input, never in arguments). Requests carry the
/// service's own origin, as it requires.
public struct ViewerClient: ViewerService {
    public let base: URL
    let origin: String
    let session: URLSession

    /// A client for `text`, which must be the service's local capability URL.
    public init?(_ text: String) {
        guard let url = URL(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
            url.scheme == "http", url.host == "127.0.0.1", let port = url.port,
            url.absoluteString.hasSuffix("/")
        else { return nil }
        base = url
        origin = "http://127.0.0.1:\(port)"
        let configuration = URLSessionConfiguration.ephemeral
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        configuration.timeoutIntervalForRequest = 8
        session = URLSession(configuration: configuration)
    }

    func request(
        _ route: String, body: (any Encodable & Sendable)? = nil, timeout: TimeInterval = 8
    ) throws -> URLRequest {
        var request = URLRequest(url: base.appendingPathComponent(route))
        request.timeoutInterval = timeout
        request.setValue(origin, forHTTPHeaderField: "Origin")
        if let body {
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONEncoder().encode(body)
        }
        return request
    }

    public func status() async throws -> ViewerStatus {
        let (data, _) = try await session.data(for: request("status.json"))
        return try JSONDecoder().decode(ViewerStatus.self, from: data)
    }

    public func frame() async throws -> Frame {
        let (data, response) = try await session.data(for: request("frame.png"))
        guard let http = response as? HTTPURLResponse else { throw ServiceError("No picture") }
        guard http.statusCode == 200 else { throw Self.failure(data, fallback: "Capture failed") }
        let header = { (name: String) in http.value(forHTTPHeaderField: name) }
        guard let id = header("X-Frame-Id"), !id.isEmpty,
            let revision = header("X-Session-Revision"), !revision.isEmpty,
            let width = header("X-Display-Width").flatMap(Double.init), width > 0,
            let height = header("X-Display-Height").flatMap(Double.init), height > 0
        else { throw ServiceError("Frame geometry is unavailable") }
        return Frame(png: data, id: id, revision: revision, display: .init(width: width, height: height))
    }

    public func acquire(frame: String) async throws -> String {
        struct Body: Encodable { let frame: String }
        struct Reply: Decodable { let lease: String }
        let data = try await post("control/acquire", Body(frame: frame))
        guard let reply = try? JSONDecoder().decode(Reply.self, from: data) else {
            throw ServiceError("Control was not granted")
        }
        return reply.lease
    }

    public func renew(lease: String, frame: String) async throws {
        struct Body: Encodable {
            let lease: String
            let frame: String
        }
        _ = try await post("control/renew", Body(lease: lease, frame: frame))
    }

    public func release(lease: String) async throws {
        struct Body: Encodable { let lease: String }
        _ = try await post("control/release", Body(lease: lease))
    }

    public func revoke() async throws {
        struct Body: Encodable {}
        _ = try await post("control/revoke", Body())
    }

    public func input(lease: String, frame: String, actions: [InputAction]) async throws {
        struct Body: Encodable {
            let lease: String
            let frame: String
            let actions: [InputAction]
        }
        _ = try await post("input", Body(lease: lease, frame: frame, actions: actions))
    }

    /// Recover the helper; whether it restarted (rather than starting input).
    public func recover() async throws -> Bool {
        struct Body: Encodable {}
        struct Reply: Decodable { let restarted: Bool? }
        let data = try await post("recover", Body(), timeout: 20)
        return (try? JSONDecoder().decode(Reply.self, from: data))?.restarted == true
    }

    /// POST `body` to `route`; the reply when it says `ok`, else its error.
    func post(_ route: String, _ body: some Encodable & Sendable, timeout: TimeInterval = 8) async throws -> Data {
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request(route, body: body, timeout: timeout))
        } catch let error as URLError where error.code == .timedOut {
            throw ServiceError(
                "The request timed out. Its outcome may be unknown; do not repeat it automatically.",
                code: "transport", uncertain: true)
        }
        let ok = (try? JSONDecoder().decode(Acknowledgement.self, from: data))?.ok == true
        guard (response as? HTTPURLResponse)?.statusCode == 200, ok else {
            throw Self.failure(data, fallback: "Request was refused")
        }
        return data
    }

    /// Every POST reply says whether it worked.
    private struct Acknowledgement: Decodable { let ok: Bool? }

    static func failure(_ data: Data, fallback: String) -> ServiceError {
        struct Reply: Decodable {
            let error: String?
            let code: String?
            let uncertain: Bool?
        }
        let reply = try? JSONDecoder().decode(Reply.self, from: data)
        return ServiceError(reply?.error ?? fallback, code: reply?.code, uncertain: reply?.uncertain == true)
    }
}
