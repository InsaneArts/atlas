import Foundation
import os

/// The app's one connection to the Atlas service, handed to every part of the app in place of a client: it sends each
/// request through the client of the service's current start, and takes a new one when the app attaches again after
/// the service restarted. Its parts never hold a client that outlived its service.
public final class ServiceConnection: ViewerService {
  /// The client of the service's current start and that start's name while attached; while detached, why.
  private let state = OSAllocatedUnfairLock<(current: ViewerClient?, incarnation: String?, why: ServiceError)>(
    initialState: (nil, nil, .detached)
  )

  /// A connection not attached yet.
  public init() {}

  /// Sends from now on through `client`, of the service start named `incarnation`.
  ///
  /// - Parameters:
  ///   - client: The new client.
  ///   - incarnation: The service start it reaches.
  public func attach(_ client: ViewerClient, incarnation: String) {
    state.withLock { $0 = (client, incarnation, .detached) }
  }

  /// Stops sending: the service is gone until the app attaches again, and each request meanwhile fails with `why`.
  ///
  /// - Parameter why: What requests say until then, such as a service of another version to restart.
  public func detach(because why: ServiceError = .detached) {
    state.withLock { state in
      state.current = nil
      state.why = why
    }
  }

  /// The service start the app is attached to; nil while detached.
  public var incarnation: String? {
    state.withLock { $0.current == nil ? nil : $0.incarnation }
  }

  /// The client of the current start, and that start.
  ///
  /// - Returns: The client and its start.
  /// - Throws: ``ServiceError`` while detached.
  private func current() throws(ServiceError) -> (ViewerClient, String?) {
    let attached = state.withLock { state in
      state.current.map { Result.success(($0, state.incarnation)) } ?? .failure(state.why)
    }
    return try attached.get()
  }

  /// Refuses a reply from a start the app is no longer attached to: what it says is that start's, not this one's.
  ///
  /// - Parameter start: The start the request went to.
  /// - Throws: ``ServiceError`` when the app attached to another start, or detached, meanwhile.
  private func still(_ start: String?) throws(ServiceError) {
    guard incarnation == start else { throw ServiceError.detached }
  }

  /// `call` through the current client, its reply kept only while the app is still attached to the same start.
  ///
  /// - Parameter call: The request.
  /// - Returns: Its reply.
  /// - Throws: What the request throws, and ``ServiceError`` while detached or after a restart.
  private func send<T>(_ call: (ViewerClient) async throws -> T) async throws -> T {
    let (client, start) = try current()
    let reply = try await call(client)
    try still(start)
    return reply
  }

  /// [`send`] for a request that throws ``ServiceError`` alone.
  ///
  /// - Parameter call: The request.
  /// - Returns: Its reply.
  /// - Throws: ``ServiceError``.
  private func sendTyped<T>(_ call: (ViewerClient) async throws(ServiceError) -> T) async throws(ServiceError) -> T {
    let (client, start) = try current()
    let reply = try await call(client)
    try still(start)
    return reply
  }

  /// The service's status.
  public func status() async throws -> ViewerStatus { try await send { try await $0.status() } }

  /// The newest frame.
  public func frame() async throws -> Frame { try await send { try await $0.frame() } }

  /// The frame service's description.
  public func framePipe() async throws -> FramePipeDescriptor? { try await send { try await $0.framePipe() } }

  /// Takes control.
  public func acquire(frame: String) async throws -> String { try await send { try await $0.acquire(frame: frame) } }

  /// Renews control.
  public func renew(lease: String, frame: String) async throws {
    try await send { try await $0.renew(lease: lease, frame: frame) }
  }

  /// Gives control back.
  public func release(lease: String) async throws { try await send { try await $0.release(lease: lease) } }

  /// Ends any control.
  public func revoke() async throws { try await send { try await $0.revoke() } }

  /// Stops or resumes agents.
  @discardableResult
  public func setStopped(_ stopped: Bool) async throws -> StopOutcome {
    try await send { try await $0.setStopped(stopped) }
  }

  /// Adds an app to the accounts.
  public func addAccount(app: String, name: String) async throws {
    try await send { try await $0.addAccount(app: app, name: name) }
  }

  /// Removes an account.
  public func removeAccount(_ account: ViewerStatus.Account) async throws {
    try await send { try await $0.removeAccount(account) }
  }

  /// Sets an account's level.
  public func setLevel(_ level: AccountLevel, of account: ViewerStatus.Account) async throws {
    try await send { try await $0.setLevel(level, of: account) }
  }

  /// The questions Atlas asks.
  public func asks() async throws(ServiceError) -> AskList {
    try await sendTyped { (client) async throws(ServiceError) in try await client.asks() }
  }

  /// Answers a question.
  public func answer(_ ask: String, with answer: AskAnswer) async throws {
    try await send { try await $0.answer(ask, with: answer) }
  }

  /// Ends what the person allowed.
  public func endAllowed(in account: AccountKey) async throws {
    try await send { try await $0.endAllowed(in: account) }
  }

  /// Starts a sign-in.
  public func startSignIn(_ kind: SignInKind, site: String, browser: String?) async throws -> SignInStarted {
    try await send { try await $0.startSignIn(kind, site: site, browser: browser) }
  }

  /// Renews a sign-in.
  public func renewSignIn(lease: String) async throws { try await send { try await $0.renewSignIn(lease: lease) } }

  /// Finishes a sign-in.
  public func finishSignIn(lease: String) async throws -> SignInEnded {
    try await send { try await $0.finishSignIn(lease: lease) }
  }

  /// Cancels a sign-in.
  public func cancelSignIn(lease: String) async throws { try await send { try await $0.cancelSignIn(lease: lease) } }

  /// Finishes or cancels an interrupted sign-in.
  public func settleInterruptedSignIn(id: String, done: Bool) async throws {
    try await send { try await $0.settleInterruptedSignIn(id: id, done: done) }
  }

  /// Sends input.
  public func input(lease: String, frame: String, actions: [InputAction]) async throws {
    try await send { try await $0.input(lease: lease, frame: frame, actions: actions) }
  }

  /// The newest activity.
  public func activity() async throws(ServiceError) -> ActivityLog {
    try await sendTyped { (client) async throws(ServiceError) in try await client.activity() }
  }

  /// One session's activity.
  public func activity(of session: String) async throws(ServiceError) -> ActivityLog {
    try await sendTyped { (client) async throws(ServiceError) in try await client.activity(of: session) }
  }

  /// The screenshots kept.
  public func screenshots() async throws(ServiceError) -> ScreenshotList {
    try await sendTyped { (client) async throws(ServiceError) in try await client.screenshots() }
  }

  /// One screenshot.
  public func screenshot(name: String, full: Bool) async throws(ServiceError) -> Data {
    try await sendTyped { (client) async throws(ServiceError) in try await client.screenshot(name: name, full: full) }
  }

  /// The host sessions.
  public func sessions() async throws(ServiceError) -> HostSessionList {
    try await sendTyped { (client) async throws(ServiceError) in try await client.sessions() }
  }

  /// Cleans up a session.
  public func cleanup(session: String, apply: Bool) async throws(ServiceError) -> CleanupReply {
    try await sendTyped { (client) async throws(ServiceError) in
      try await client.cleanup(session: session, apply: apply)
    }
  }
}
