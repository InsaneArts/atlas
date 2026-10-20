import AtlasKit
import Foundation

/// Control: the lease this window holds, its heartbeat, and the input that
/// belongs to it. The service checks every action against the lease and
/// the picture it was aimed at; this side only stops sending early.
extension ViewerController {
  /// The lease is renewed every this many heartbeat ticks: 5, every 2.5 s at ``heartbeatEvery``.
  static let renewEvery = 5

  /// Renews the lease now and then, and ends control once the picture is stale, whether or not the person is still
  /// giving input.
  ///
  /// - Parameter granted: The lease just granted.
  func startHeartbeat(_ granted: String) {
    heartbeat = Task { [weak self] in
      var tick = 0
      while !Task.isCancelled {
        guard let interval = self?.heartbeatInterval else { return }
        try? await Task.sleep(for: interval)
        guard !Task.isCancelled, let self, session.lease == granted else { return }
        guard !pictureStale else {
          releaseControl(ViewerWords.stalePicture)
          return
        }
        tick += 1
        if tick % Self.renewEvery == 0 { await renew(granted) }
      }
    }
  }

  /// Renews the lease, unless it has ended; a failed renewal ends control.
  ///
  /// - Parameter owner: The lease to renew.
  private func renew(_ owner: String) async {
    guard session.lease == owner, let frame = reading.frame else { return }
    do {
      try await service.renew(lease: owner, frame: frame.id)
    } catch {
      if session.lease == owner { releaseControl(error.localizedDescription) }
    }
  }

  /// The window stopped taking the keyboard: control goes back.
  func lostFocus() {
    focused = false
    releaseControl()
  }

  /// The window takes the keyboard again; control waits to be taken.
  func gainedFocus() {
    focused = true
  }

  /// The last picture is too old to aim input at.
  var pictureStale: Bool {
    guard let frameAt = reading.frameAt else { return true }
    return clock() - frameAt >= Freshness.stale
  }

  /// Whether input may go to the helper now. Only the heartbeat ends
  /// control over a stale picture; this side just stops sending.
  var canInput: Bool {
    session.lease != nil && reading.frame != nil && !session.paused && focused && shown && !pictureStale
  }

  /// Queues input for the helper, aimed at the picture on screen, while input may go.
  ///
  /// - Parameter actions: The actions, in order.
  func enqueue(_ actions: [InputAction]) {
    guard canInput, let frame = reading.frame else { return }
    input.enqueue(actions, frame: frame.id)
  }

  /// Queues input that undoes what earlier input holds down, a button's release, whenever control is held: over a
  /// stale picture too, which the service may refuse, and then control ends. With no picture to aim at, control
  /// ends at once, and the service's release lifts what is held.
  ///
  /// - Parameter actions: The actions, in order.
  func enqueueCleanup(_ actions: [InputAction]) {
    guard session.lease != nil else { return }
    guard let frame = reading.frame else { return releaseControl() }
    input.enqueue(actions, frame: frame.id)
  }
}
