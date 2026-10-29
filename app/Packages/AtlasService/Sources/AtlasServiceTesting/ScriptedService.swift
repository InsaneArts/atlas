import AtlasService
import Foundation
import Observation
import Testing

/// A stand-in viewer service: answers from fixed replies, can hold or fail a request until told, and records what it
/// was asked.
/// It is main-actor bound, as every test that uses it is, so its state is only ever touched there, and observable, so
/// a test's `settle` hears each request it records. Each request is marked `@MainActor` too: a witness of a
/// requirement that runs on its caller's actor takes that isolation otherwise, even in a main-actor class.
@MainActor
@Observable
public final class ScriptedService: ViewerService {
  /// The status the service reports, as `status.json`'s fields.
  public var reply: [String: Any] = ["ready": true, "canObserve": true, "overview": ["mode": "observing"]]
  /// The helper's session and display revision each picture carries.
  public var revision = "r1"
  /// Each picture's PNG.
  public var picture = Data()
  /// The helper's display each picture was taken of, in points.
  public var display = ViewerStatus.Size(width: 1512, height: 982)
  /// Requests to hold the next time they are made, until `resume`d.
  public var hold: Set<String> = []
  /// Holds the next request it says yes to, given its route as the request is made, until `resume`d, then is cleared:
  /// for a test that must hold one caller's request among others to the same route. Nil holds none this way.
  public var holdWhen: (@MainActor (String) -> Bool)?
  /// The requests held now, by route.
  public var held: [String: CheckedContinuation<Void, Never>] = [:]
  /// Requests that fail every time.
  public var fail: Set<String> = []
  /// What every Stop or Resume answers.
  public var stopOutcome = StopOutcome()
  /// How many acquisitions from now on are refused as busy, as while an agent's action runs.
  public var busyAcquires = 0
  /// How many acquisitions from now on are granted, but their reply is lost on the way back, as a timeout loses it.
  public var lostAcquireReplies = 0
  /// What every release answers from now on, as the service refuses it; nil lets it go through.
  public var releaseRefusal: ServiceError?
  /// Whether the service is there but refuses to give its status, as a busy one does.
  public var busy = false
  /// Status requests made.
  public var statusCalls = 0
  /// Status requests answered, held or failed ones included.
  public var answered = 0
  /// Picture requests made.
  public var frameCalls = 0
  /// The control and input requests made, in order.
  public var log: [String] = []
  /// The input sent, a batch a request.
  public var actions: [[InputAction]] = []
  /// Whether a viewer holds the lease: granted, and not released or revoked since; nil until one is first granted.
  /// The status says so as `controlActive` once known, unless its reply sets that itself.
  public var leaseHeld: Bool?
  /// What was done in the helper lately, as the activity reply lists it.
  public var activityLog = ActivityLog()
  /// The apps open in the helper, front to back.
  public var openApps: [HelperApp] = []
  /// The screenshots agents took, newest first.
  public var shots: [Screenshot] = []
  /// The folder the screenshots are kept in; nil names none.
  public var shotsFolder: URL?
  /// Each screenshot's bytes by name, over ``shot``.
  public var shotData: [String: Data] = [:]
  /// Whether every screenshot request waits, in the order asked, until ``resumeShot(name:full:)`` lets it answer.
  public var holdShots = false
  /// The screenshot requests held so far, oldest first.
  public var heldShots: [HeldShot] = []
  /// Screenshot requests made.
  public var shotCalls = 0
  /// Each screenshot's bytes, whichever size is asked for.
  public var shot = Data()
  /// Activity requests made.
  public var activityCalls = 0
  /// The host sessions `sessions.json` lists.
  public var sessionList = HostSessionList()
  /// Whether the service offers `sessions.json`; an older one answers ``ServiceError/notOffered``.
  public var offersSessions = true
  /// Session list requests made.
  public var sessionsCalls = 0
  /// What a cleanup preview answers.
  public var cleanupPreview = CleanupReply(ok: true, preview: true)
  /// What a cleanup apply answers.
  public var cleanupApplied = CleanupReply(ok: true, state: .complete)
  /// Whether a cleanup apply gets no reply, as one that runs past the client's wait does.
  public var cleanupUnanswered = false
  /// The cleanup requests made, in order: "preview KEY" or "apply KEY".
  public var cleanups: [String] = []
  /// Apps requests made.
  var appsCalls = 0

  /// A service that answers every request at once, ready and observing.
  public init() {}

  /// Whether any viewer's control was revoked.
  public var revoked: Bool { log.contains("revoke") }

  /// Lets the oldest held screenshot request for `name` at that size answer.
  ///
  /// - Parameters:
  ///   - name: The screenshot asked for.
  ///   - full: Whether it was asked for as taken.
  public func resumeShot(name: String, full: Bool) {
    guard let index = heldShots.firstIndex(where: { $0.name == name && $0.full == full && !$0.answered }) else {
      return
    }
    heldShots[index].answered = true
    heldShots[index].resume.resume()
  }

  /// Lets every held screenshot request still waiting answer, so none is left at a test's end.
  public func resumeAllShots() {
    for index in heldShots.indices where !heldShots[index].answered {
      heldShots[index].answered = true
      heldShots[index].resume.resume()
    }
  }

  /// Lets a held request answer.
  ///
  /// - Parameter route: The request's route.
  public func resume(_ route: String) {
    let continuation = held.removeValue(forKey: route)
    continuation?.resume()
  }

  /// Holds a request when asked to, else lets it wait one turn as the live client's network request does, then fails
  /// it when asked to.
  ///
  /// - Parameter route: The request's route.
  /// - Throws: A ``ServiceError`` saying the route failed.
  private func pause(_ route: String) async throws(ServiceError) {
    let chosen = holdWhen?(route) == true
    if chosen { holdWhen = nil }
    if hold.remove(route) != nil || chosen {
      await withCheckedContinuation { holding($0, route) }
    } else {
      // The live client suspends on every request, so an unheld one never answers in line either.
      await Task.yield()
    }
    if fail.contains(route) { throw ServiceError("\(route) failed") }
  }

  /// Keeps `continuation` as `route`'s held request. A second one held on the same route is a test's mistake: it is
  /// recorded as an issue, and the first is let go rather than lost.
  ///
  /// - Parameters:
  ///   - continuation: The held request.
  ///   - route: Its route.
  private func holding(_ continuation: CheckedContinuation<Void, Never>, _ route: String) {
    if let earlier = held.updateValue(continuation, forKey: route) {
      Issue.record("A second \(route) request was held while one already was; the first was let go")
      earlier.resume()
    }
  }

  /// Answers `reply` as the status, with `leaseHeld` as its control where `reply` names none, unless held, failed or
  /// busy.
  ///
  /// - Returns: The status `reply` describes.
  /// - Throws: A ``ServiceError`` when the route fails, or ``StatusRefused`` while `busy`.
  @MainActor
  public func status() async throws -> ViewerStatus {
    statusCalls += 1
    defer { answered += 1 }
    // Read when asked, as the service does: a held reply still says what was so then.
    let answer = reply.merging(leaseHeld.map { ["controlActive": $0] } ?? [:]) { given, _ in given }
    try await pause("status")
    if busy { throw StatusRefused("Viewer is busy") }
    return try AtlasServiceTesting.status(answer)
  }

  /// Answers `picture` as the helper's desktop, unless held or failed.
  ///
  /// - Returns: The picture, with `revision` and `display`.
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  public func frame() async throws -> Frame {
    frameCalls += 1
    try await pause("frame")
    return Frame(
      png: picture,
      id: "frame-\(statusCalls)",
      revision: revision,
      display: display
    )
  }

  /// Grants control, unless held, failed, refused as busy or its reply is lost.
  ///
  /// - Parameter frame: The id of the picture the person acts on.
  /// - Returns: The lease, `lease`.
  /// - Throws: A ``ServiceError`` when the route fails or is busy, or a timeout when its reply is lost.
  @MainActor
  public func acquire(frame: String) async throws -> String {
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

  /// Records the renewal, unless held or failed.
  ///
  /// - Parameters:
  ///   - lease: The lease to renew.
  ///   - frame: The id of the picture now shown.
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  public func renew(lease: String, frame: String) async throws {
    log.append("renew")
    try await pause("renew")
  }

  /// Gives control back, unless held, failed or refused by `releaseRefusal`.
  ///
  /// - Parameter lease: The lease to end.
  /// - Throws: A ``ServiceError`` when the route fails, or `releaseRefusal`.
  @MainActor
  public func release(lease: String) async throws {
    log.append("release \(lease)")
    try await pause("release")
    if let releaseRefusal { throw releaseRefusal }
    leaseHeld = false
    log.append("released \(lease)")
  }

  /// Ends any viewer's control, unless held or failed.
  ///
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  public func revoke() async throws {
    log.append("revoke")
    try await pause("revoke")
    leaseHeld = false
  }

  /// Records stopping or resuming agents, unless held or failed.
  ///
  /// - Parameter stopped: Whether agents stop.
  /// - Returns: ``stopOutcome``.
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  @discardableResult
  public func setStopped(_ stopped: Bool) async throws -> StopOutcome {
    log.append(stopped ? "stop" : "resume")
    try await pause("hold")
    return stopOutcome
  }

  /// Records the input, a batch a request, unless held or failed.
  ///
  /// - Parameters:
  ///   - lease: The lease the input belongs to.
  ///   - frame: The id of the picture the input's coordinates refer to.
  ///   - actions: The actions, in order.
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  public func input(lease: String, frame: String, actions: [InputAction]) async throws {
    log.append("input \(lease) \(actions.count)")
    self.actions.append(actions)
    try await pause("input")
  }

  /// Records the recovery, which always restarts the helper.
  ///
  /// - Returns: True.
  @MainActor
  public func recover() async throws -> Bool {
    log.append("recover")
    return true
  }

  /// Answers `activityLog`, unless held or failed.
  ///
  /// - Returns: The activity log.
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  public func activity() async throws(ServiceError) -> ActivityLog {
    activityCalls += 1
    try await pause("activity")
    return activityLog
  }

  /// Answers `openApps`, unless held or failed.
  ///
  /// - Returns: The apps open in the helper.
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  public func apps() async throws(ServiceError) -> [HelperApp] {
    appsCalls += 1
    try await pause("apps")
    return openApps
  }

  /// Answers `shots` and `shotsFolder`, unless held or failed.
  ///
  /// - Returns: The screenshots and their folder.
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  public func screenshots() async throws(ServiceError) -> ScreenshotList {
    try await pause("screenshots")
    return ScreenshotList(shots: shots, folder: shotsFolder)
  }

  /// Answers a screenshot's bytes, from `shotData` or else `shot`, held while `holdShots` until resumed.
  ///
  /// - Parameters:
  ///   - name: The screenshot asked for.
  ///   - full: Whether it is asked for as taken, rather than as a thumbnail.
  /// - Returns: Its bytes.
  /// - Throws: A ``ServiceError`` when the route fails.
  @MainActor
  public func screenshot(name: String, full: Bool) async throws(ServiceError) -> Data {
    // Read when asked, as the service does: a held reply still carries the bytes there were then.
    shotCalls += 1
    let answer = shotData[name] ?? shot
    if holdShots {
      await withCheckedContinuation { heldShots.append(HeldShot(name: name, full: full, resume: $0)) }
    }
    try await pause("screenshot")
    return answer
  }

  /// Answers `sessionList`, or ``ServiceError/notOffered`` unless `offersSessions`, unless held or failed.
  ///
  /// - Returns: The host sessions.
  /// - Throws: A ``ServiceError`` when the route fails or is not offered.
  @MainActor
  public func sessions() async throws(ServiceError) -> HostSessionList {
    sessionsCalls += 1
    try await pause("sessions")
    guard offersSessions else { throw .notOffered }
    return sessionList
  }

  /// Answers `cleanupPreview` or `cleanupApplied`, unless held or failed: a preview's route is `cleanup.json`, an
  /// apply's `cleanup`.
  ///
  /// - Parameters:
  ///   - session: The session's key.
  ///   - apply: Whether it cleans up, rather than reads what it would take.
  /// - Returns: The scripted reply.
  /// - Throws: A ``ServiceError`` when the route fails, or one that got no reply while `cleanupUnanswered`.
  @MainActor
  public func cleanup(session: String, apply: Bool) async throws(ServiceError) -> CleanupReply {
    cleanups.append("\(apply ? "apply" : "preview") \(session)")
    try await pause(apply ? "cleanup" : "cleanup.json")
    if apply, cleanupUnanswered {
      throw ServiceError("cleanup got no reply", code: ViewerClient.unansweredCleanupCode)
    }
    return apply ? cleanupApplied : cleanupPreview
  }
}
