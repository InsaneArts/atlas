import AtlasKit
import Foundation
import Testing

/// Wait until `done` holds, for models that work in the background; after
/// two seconds the test fails with `message`.
@MainActor
func settle(_ message: String = "the condition never held", _ done: () -> Bool) async {
    let deadline = ContinuousClock.now + .seconds(2)
    while !done() {
        guard ContinuousClock.now < deadline else {
            Issue.record(Comment(rawValue: message))
            return
        }
        try? await Task.sleep(for: .milliseconds(5))
    }
}

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

/// A stand-in viewer service: answers from fixed replies, can hold or fail
/// a request until told, and records what it was asked.
@MainActor
final class FakeService: ViewerService, @unchecked Sendable {
    var reply: [String: Any] = ["ready": true, "canObserve": true, "overview": ["mode": "observing"]]
    var revision = "r1"
    var picture = Data()
    /// Requests to hold the next time they are made, until `resume`d.
    var hold: Set<String> = []
    var held: [String: CheckedContinuation<Void, Never>] = [:]
    /// Requests that fail every time.
    var fail: Set<String> = []
    var statusCalls = 0
    /// Status requests answered, held or failed ones included.
    var answered = 0
    var log: [String] = []
    var actions: [[InputAction]] = []

    var revoked: Bool { log.contains("revoke") }

    func resume(_ route: String) {
        held.removeValue(forKey: route)?.resume()
    }

    private func pause(_ route: String) async throws {
        if hold.remove(route) != nil {
            await withCheckedContinuation { held[route] = $0 }
        }
        if fail.contains(route) { throw ServiceError("\(route) failed") }
    }

    func status() async throws -> ViewerStatus {
        statusCalls += 1
        defer { answered += 1 }
        try await pause("status")
        return try JSONDecoder().decode(ViewerStatus.self, from: JSONSerialization.data(withJSONObject: reply))
    }

    func frame() async throws -> Frame {
        try await pause("frame")
        return Frame(
            png: picture, id: "frame-\(statusCalls)", revision: revision, display: .init(width: 1512, height: 982))
    }

    func acquire(frame: String) async throws -> String {
        log.append("acquire \(frame)")
        try await pause("acquire")
        return "lease"
    }

    func renew(lease: String, frame: String) async throws {
        log.append("renew")
        try await pause("renew")
    }

    func release(lease: String) async throws { log.append("release \(lease)") }
    func revoke() async throws { log.append("revoke") }

    func input(lease: String, frame: String, actions: [InputAction]) async throws {
        log.append("input \(lease) \(actions.count)")
        self.actions.append(actions)
        try await pause("input")
    }

    func recover() async throws -> Bool {
        log.append("recover")
        return true
    }
}
