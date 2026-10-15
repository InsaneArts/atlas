import AtlasKit
import Foundation

/// A stand-in command line: answers by the first arguments, and records
/// what ran and what it was given.
final class FakeCommands: CommandRunning, @unchecked Sendable {
    private let lock = NSLock()
    private var answers: [String: String]
    private var log: [([String], Data?)] = []

    init(_ answers: [String: String]) {
        self.answers = answers
    }

    var ran: [[String]] { lock.withLock { log.map(\.0) } }
    var inputs: [Data?] { lock.withLock { log.map(\.1) } }

    func answer(_ key: String, _ reply: String) {
        lock.withLock { answers[key] = reply }
    }

    func run(_ arguments: [String], input: Data?) async -> Outcome {
        let reply = lock.withLock {
            log.append((arguments, input))
            return answers[arguments.prefix(2).joined(separator: " ")] ?? answers[arguments.first ?? ""]
        }
        guard let reply else { return Outcome(ok: false, reply: Data(), message: "unknown command") }
        return Outcome.parse(status: 0, stdout: Data(reply.utf8), stderr: Data())
    }
}

/// A stand-in viewer service.
final class FakeService: ViewerService, @unchecked Sendable {
    let base = URL(string: "http://127.0.0.1:1/x/")!
    var reply: [String: Any] = [:]
    var revoked = false

    func status() async throws -> ViewerStatus {
        try JSONDecoder().decode(ViewerStatus.self, from: JSONSerialization.data(withJSONObject: reply))
    }
    func frame() async throws -> Frame { throw ServiceError("Capture failed") }
    func acquire(frame: String) async throws -> String { "lease" }
    func renew(lease: String, frame: String) async throws {}
    func release(lease: String) async throws {}
    func revoke() async throws { revoked = true }
    func input(lease: String, frame: String, actions: [InputAction]) async throws {}
    func recover() async throws -> Bool { true }
}
