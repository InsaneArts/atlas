import Foundation

/// The local viewer, reached only through its capability URL. The URL
/// arrives on stdin, never in argv.
public final class Viewer: @unchecked Sendable {
    public let base: URL
    let origin: String
    let session: URLSession

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

    private func request(_ route: String, post: Bool = false) -> URLRequest {
        var request = URLRequest(url: base.appendingPathComponent(route))
        request.setValue(origin, forHTTPHeaderField: "Origin")
        if post {
            request.httpMethod = "POST"
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = Data("{}".utf8)
        }
        return request
    }

    /// The viewer's status report, delivered on the main queue.
    public func status(_ done: @escaping @MainActor @Sendable ([String: Any]?) -> Void) {
        session.dataTask(with: request("status.json")) { data, _, _ in
            let object = data.flatMap { try? JSONSerialization.jsonObject(with: $0) }
            let status = object as? [String: Any]
            Task { @MainActor in done(status) }
        }.resume()
    }

    /// The latest helper desktop frame (PNG bytes), on the main queue.
    public func frame(_ done: @escaping @MainActor @Sendable (Data?) -> Void) {
        session.dataTask(with: request("frame.png")) { data, response, _ in
            let ok = (response as? HTTPURLResponse)?.statusCode == 200
            let frame = ok ? data : nil
            Task { @MainActor in done(frame) }
        }.resume()
    }

    /// Take control back from the viewer; nil on success, else why not.
    public func revoke(_ done: @escaping @MainActor @Sendable (String?) -> Void) {
        session.dataTask(with: request("control/revoke", post: true)) { data, _, error in
            let object = data.flatMap { try? JSONSerialization.jsonObject(with: $0) }
            let reply = object as? [String: Any]
            let failure =
                reply?["ok"] as? Bool == true
                ? nil : (reply?["error"] as? String ?? error?.localizedDescription ?? "Failed")
            Task { @MainActor in done(failure) }
        }.resume()
    }
}
