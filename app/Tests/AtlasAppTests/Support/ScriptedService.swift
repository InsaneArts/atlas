import AtlasKit
import Foundation

/// A stand-in viewer service: answers from fixed replies, can hold or fail a request until told, and records what it
/// was asked.
/// It is main-actor bound, as every test that uses it is, so its state is only ever touched there.
@MainActor
final class ScriptedService: ViewerService {
  /// The status the service reports, as `status.json`'s fields.
  var reply: [String: Any] = ["ready": true, "canObserve": true, "overview": ["mode": "observing"]]
  /// The helper's session and display revision each picture carries.
  var revision = "r1"
  /// Each picture's PNG.
  var picture = Data()
  /// Requests to hold the next time they are made, until `resume`d.
  var hold: Set<String> = []
  /// The requests held now, by route.
  var held: [String: CheckedContinuation<Void, Never>] = [:]
  /// Requests that fail every time.
  var fail: Set<String> = []
  /// Whether the service is there but refuses to give its status, as a busy one does.
  var busy = false
  /// Status requests made.
  var statusCalls = 0
  /// Status requests answered, held or failed ones included.
  var answered = 0
  /// Picture requests made.
  var frameCalls = 0
  /// The control and input requests made, in order.
  var log: [String] = []
  /// The input sent, a batch a request.
  var actions: [[InputAction]] = []

  /// Whether any viewer's control was revoked.
  var revoked: Bool { log.contains("revoke") }

  /// Lets a held request answer.
  ///
  /// - Parameter route: The request's route.
  func resume(_ route: String) {
    let continuation = held.removeValue(forKey: route)
    continuation?.resume()
  }

  /// Holds a request when asked to, then fails it when asked to.
  ///
  /// - Parameter route: The request's route.
  /// - Throws: A ``ServiceError`` saying the route failed.
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
    if busy { throw StatusRefused("Viewer is busy") }
    return try AtlasAppTests.status(reply)
  }

  func frame() async throws -> Frame {
    frameCalls += 1
    try await pause("frame")
    return Frame(
      png: picture,
      id: "frame-\(statusCalls)",
      revision: revision,
      display: .init(width: 1512, height: 982)
    )
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

  func revoke() async throws {
    log.append("revoke")
    try await pause("revoke")
  }

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
