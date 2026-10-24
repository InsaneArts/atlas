import AtlasService
import Observation

/// The app's one record of control work the service has not answered yet: acquisitions awaiting their reply, an
/// acquisition whose reply was lost (held until a lease it may have granted can no longer be alive), the releases that
/// give a lease back (a late grant's compensating release among them), and releases the service refused, which stay
/// pending and are sent again before anything else that needs the helper free. It outlives any
/// window, so closing or reopening the main window never drops a release still under way. Take Control waits while
/// anything is under way here; the helper's own commands wait for all of it to be answered first.
@Observable
final class ControlBarrier {
  /// 10 s: how long a lease the service grants stays alive unrenewed (`LEASE_SECONDS`, src/lib/viewer.rs).
  static let leaseLifetime: Duration = .seconds(10)
  /// 8 s: the client's request timeout (`ViewerClient.timeout`). The service runs every request to its end after the
  /// client gives up (src/lib/viewer.rs, `handle`), so a lost acquisition may still be granted after its reply was
  /// given up on; this bounds how much later, short of a service that hangs, which the status check below covers.
  static let lateGrant: Duration = .seconds(8)
  /// 1 s: between the status readings that end the hold on a lost acquisition, as often as the panel's own.
  static let statusEvery: Duration = .seconds(1)
  /// 10: the status readings tried once a lost acquisition's lease can no longer be alive, a second apart, before
  /// the hold gives up and the next command is refused, saying why: about the client's own request timeout again, so
  /// a service that is only slow gets its answer in.
  static let statusTries = 10

  /// Acquisitions and releases sent and not answered yet, and lost acquisitions still held.
  private(set) var outstanding = 0
  /// How many wait for every outstanding reply now, so a test knows a command reached its wait.
  private(set) var waiters = 0
  /// Leases whose release the service refused, sent again before the next command.
  private(set) var unreleased: [String] = []
  /// Why a lost acquisition could not be confirmed over: no status reading answered once its lease could no longer
  /// be alive. The next command reads the status once more and is refused, saying this, until one answers.
  private(set) var unconfirmed: String?
  /// The viewer service every release goes to.
  @ObservationIgnored private let service: any ViewerService
  /// The clock a lost acquisition's hold waits on, and the status readings that end it.
  @ObservationIgnored private let clock: any Clock<Duration>
  /// Those waiting for every outstanding reply, by the number each wait took.
  @ObservationIgnored private var waiting: [Int: CheckedContinuation<Void, Never>] = [:]
  /// The number the next wait takes.
  @ObservationIgnored private var nextWait = 0

  /// Creates a barrier with nothing under way.
  ///
  /// - Parameters:
  ///   - service: The viewer service every release goes to, and whose status ends a lost acquisition's hold.
  ///   - clock: The clock a lost acquisition's hold waits on; the app's.
  init(service: any ViewerService, clock: any Clock<Duration>) {
    self.service = service
    self.clock = clock
  }

  /// Whether control work is under way, so Take Control waits.
  var isBusy: Bool { outstanding > 0 }

  /// Counts `work`, an acquisition's request and whatever it must undo, until it ends.
  ///
  /// - Parameter work: The work.
  /// - Returns: What the work returns.
  /// - Throws: What the work throws.
  func tracking<T>(_ work: () async throws -> T) async rethrows -> T {
    begin()
    defer { end() }
    return try await work()
  }

  /// Holds the barrier for an acquisition whose outcome is unknown: its reply was lost, timed out or cancelled, so the
  /// service may have granted control, or still may. It is held until a lease granted then can no longer be alive
  /// (``lateGrant`` and ``leaseLifetime``) and a status reading has answered since, which a service that is not hung
  /// gives. Another viewer's control, such as an agent pane's, never prolongs it, and none is ever revoked for it.
  /// Should no reading answer within ``statusTries``, the hold ends and the next command is refused, saying why
  /// (``unconfirmed``).
  func holdLostAcquisition() {
    begin()
    // Unstructured and never cancelled: the hold ends only once a lease granted meanwhile can no longer be alive.
    Task {
      try? await clock.sleep(for: Self.lateGrant + Self.leaseLifetime)
      unconfirmed = await answered(tries: Self.statusTries) ? nil : HelperWords.helperUnconfirmed
      end()
    }
  }

  /// Whether a status reading answers within `tries`, a ``statusEvery`` apart.
  ///
  /// - Parameter tries: The readings to try.
  /// - Returns: Whether one answered.
  private func answered(tries: Int) async -> Bool {
    for attempt in 0..<tries {
      if attempt > 0 { try? await clock.sleep(for: Self.statusEvery) }
      if (try? await service.status()) != nil { return true }
    }
    return false
  }

  /// Gives `lease` back, counted from now until the service answers. A refusal keeps the lease pending, to be sent
  /// again by ``settle()``.
  ///
  /// - Parameters:
  ///   - lease: The lease to end.
  ///   - answered: Runs once the service answered, with why it refused, or nil once it agreed.
  func release(_ lease: String, answered: @escaping @MainActor (String?) -> Void = { _ in }) {
    begin()
    // Unstructured, so the release is counted and sent at once while the caller goes on; `settle()` waits for it.
    Task {
      let refusal = await send(lease)
      end()
      answered(refusal)
    }
  }

  /// Gives `lease` back and waits for the answer, counted meanwhile; a refusal keeps it pending.
  ///
  /// - Parameter lease: The lease to end.
  /// - Returns: Why the service refused, or nil once it agreed.
  func releaseNow(_ lease: String) async -> String? {
    await tracking { await send(lease) }
  }

  /// Waits until every acquisition and release under way is answered, then sends again each release the service
  /// refused before. A wait that is cancelled ends at once, saying so, so a command still waiting here can be called
  /// off before it runs; one after a lost acquisition nothing confirmed reads the status once more first.
  ///
  /// - Returns: Why the helper is not free yet (a release still refused, an unconfirmed lost acquisition, or the
  ///   wait's cancellation), or nil once nothing of this app's control is left in the helper.
  func settle() async -> String? {
    while outstanding > 0 {
      if Task.isCancelled { return HelperWords.cancelled }
      waiters += 1
      let number = nextWait
      nextWait += 1
      await withTaskCancellationHandler {
        await withCheckedContinuation { waiting[number] = $0 }
      } onCancel: {
        // Unstructured, as a cancellation handler is synchronous and may run off the main actor: the wait ends there.
        Task { @MainActor in self.wake(number) }
      }
      waiters -= 1
    }
    if Task.isCancelled { return HelperWords.cancelled }
    if unconfirmed != nil {
      guard await answered(tries: 1) else { return unconfirmed }
      unconfirmed = nil
    }
    var refusal: String?
    for lease in unreleased {
      refusal = await releaseNow(lease) ?? refusal
    }
    return refusal
  }

  /// Sends one release, keeping the lease pending while its outcome is unknown. The service saying the lease is lost
  /// (`lease-lost`, src/lib/control.rs) is a confirmed answer: that lease already ended, and this app holds nothing.
  ///
  /// - Parameter lease: The lease to end.
  /// - Returns: Why it was refused, or nil once it was answered.
  private func send(_ lease: String) async -> String? {
    do {
      try await service.release(lease: lease)
    } catch let refusal as ServiceError where refusal.code == Self.leaseLost {
      // Settled below, as an agreed release is.
    } catch {
      if !unreleased.contains(lease) { unreleased.append(lease) }
      return error.localizedDescription
    }
    unreleased.removeAll { $0 == lease }
    return nil
  }

  /// The service's code for a lease that has already ended, so this app holds nothing of it.
  static let leaseLost = "lease-lost"

  /// Counts one piece of work under way.
  private func begin() {
    outstanding += 1
  }

  /// Counts one piece of work answered, and lets the waiting go on once nothing is under way.
  private func end() {
    outstanding -= 1
    guard outstanding == 0 else { return }
    let ready = waiting.values
    waiting = [:]
    for continuation in ready {
      continuation.resume()
    }
  }

  /// Ends one wait early, as its task was cancelled; one already ended is left be.
  ///
  /// - Parameter number: The number the wait took.
  private func wake(_ number: Int) {
    waiting.removeValue(forKey: number)?
      .resume()
  }
}
