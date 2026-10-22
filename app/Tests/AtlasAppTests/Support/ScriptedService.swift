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
  /// The helper's display each picture was taken of, in points.
  var display = ViewerStatus.Size(width: 1512, height: 982)
  /// Requests to hold the next time they are made, until `resume`d.
  var hold: Set<String> = []
  /// The requests held now, by route.
  var held: [String: CheckedContinuation<Void, Never>] = [:]
  /// Requests that fail every time.
  var fail: Set<String> = []
  /// How many acquisitions from now on are refused as busy, as while an agent's action runs.
  var busyAcquires = 0
  /// How many acquisitions from now on are granted, but their reply is lost on the way back, as a timeout loses it.
  var lostAcquireReplies = 0
  /// What every release answers from now on, as the service refuses it; nil lets it go through.
  var releaseRefusal: ServiceError?
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
  /// Whether a viewer holds the lease: granted, and not released or revoked since; nil until one is first granted.
  /// The status says so as `controlActive` once known, unless its reply sets that itself.
  var leaseHeld: Bool?
  /// What was done in the helper lately, as the activity reply lists it.
  var activityLog = ActivityLog()
  /// The apps open in the helper, front to back.
  var openApps: [HelperApp] = []
  /// The screenshots agents took, newest first.
  var shots: [Screenshot] = []
  /// The folder the screenshots are kept in; nil names none.
  var shotsFolder: URL?
  /// Each screenshot's bytes by name, over ``shot``.
  var shotData: [String: Data] = [:]
  /// Whether every screenshot request waits, in the order asked, until ``resumeShot(name:full:)`` lets it answer.
  var holdShots = false
  /// The screenshot requests held so far, oldest first.
  var heldShots: [HeldShot] = []
  /// Screenshot requests made.
  var shotCalls = 0
  /// Each screenshot's bytes, whichever size is asked for.
  var shot = Data()
  /// Activity requests made.
  var activityCalls = 0
  /// Apps requests made.
  var appsCalls = 0

  /// Whether any viewer's control was revoked.
  var revoked: Bool { log.contains("revoke") }

  /// Lets the oldest held screenshot request for `name` at that size answer.
  ///
  /// - Parameters:
  ///   - name: The screenshot asked for.
  ///   - full: Whether it was asked for as taken.
  func resumeShot(name: String, full: Bool) {
    guard let index = heldShots.firstIndex(where: { $0.name == name && $0.full == full && !$0.answered }) else {
      return
    }
    heldShots[index].answered = true
    heldShots[index].resume.resume()
  }

  /// Lets every held screenshot request still waiting answer, so none is left at a test's end.
  func resumeAllShots() {
    for index in heldShots.indices where !heldShots[index].answered {
      heldShots[index].answered = true
      heldShots[index].resume.resume()
    }
  }

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
  private func pause(_ route: String) async throws(ServiceError) {
    if hold.remove(route) != nil {
      await withCheckedContinuation { held[route] = $0 }
    }
    if fail.contains(route) { throw ServiceError("\(route) failed") }
  }

  func status() async throws -> ViewerStatus {
    statusCalls += 1
    defer { answered += 1 }
    // Read when asked, as the service does: a held reply still says what was so then.
    let answer = reply.merging(leaseHeld.map { ["controlActive": $0] } ?? [:]) { given, _ in given }
    try await pause("status")
    if busy { throw StatusRefused("Viewer is busy") }
    return try AtlasAppTests.status(answer)
  }

  func frame() async throws -> Frame {
    frameCalls += 1
    try await pause("frame")
    return Frame(
      png: picture,
      id: "frame-\(statusCalls)",
      revision: revision,
      display: display
    )
  }

  func acquire(frame: String) async throws -> String {
    log.append("acquire \(frame)")
    try await pause("acquire")
    if busyAcquires > 0 {
      busyAcquires -= 1
      throw ServiceError("Another Atlas controller is active", code: "busy")
    }
    leaseHeld = true
    if lostAcquireReplies > 0 {
      lostAcquireReplies -= 1
      throw URLError(.timedOut)
    }
    return "lease"
  }

  func renew(lease: String, frame: String) async throws {
    log.append("renew")
    try await pause("renew")
  }

  func release(lease: String) async throws {
    log.append("release \(lease)")
    try await pause("release")
    if let releaseRefusal { throw releaseRefusal }
    leaseHeld = false
    log.append("released \(lease)")
  }

  func revoke() async throws {
    log.append("revoke")
    try await pause("revoke")
    leaseHeld = false
  }

  func setStopped(_ stopped: Bool) async throws {
    log.append(stopped ? "stop" : "resume")
    try await pause("hold")
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

  func activity() async throws(ServiceError) -> ActivityLog {
    activityCalls += 1
    try await pause("activity")
    return activityLog
  }

  func apps() async throws(ServiceError) -> [HelperApp] {
    appsCalls += 1
    try await pause("apps")
    return openApps
  }

  func screenshots() async throws(ServiceError) -> ScreenshotList {
    try await pause("screenshots")
    return ScreenshotList(shots: shots, folder: shotsFolder)
  }

  func screenshot(name: String, full: Bool) async throws(ServiceError) -> Data {
    // Read when asked, as the service does: a held reply still carries the bytes there were then.
    shotCalls += 1
    let answer = shotData[name] ?? shot
    if holdShots {
      await withCheckedContinuation { heldShots.append(HeldShot(name: name, full: full, resume: $0)) }
    }
    try await pause("screenshot")
    return answer
  }
}
