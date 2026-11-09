import AtlasService

/// A held sign-in: the person's lease the service granted with the hold, which input uses as control's own, renewed
/// with no picture every ``ViewerControl/renewEvery`` heartbeats, whatever the window shows, until Done or Cancel ends
/// it at the service. Nothing here gives the lease back: the service ends it with the hold.
extension ViewerControl {
  /// Holds the sign-in's lease as control: input goes while the window takes it, and the lease is renewed from now
  /// on. Control held before was given back first (``ViewerController/startSignIn(_:site:browser:)``).
  ///
  /// - Parameter lease: The sign-in's lease.
  func adoptSignIn(_ lease: String) {
    holdSignIn(lease)
    heartbeat = Task { [weak self, clock, heartbeatInterval] in
      var tick = 0
      while !Task.isCancelled {
        // A cancelled heartbeat ends at once; the guard below ends the loop.
        try? await clock.sleep(for: heartbeatInterval)
        guard !Task.isCancelled, let self, signInLease == lease else { return }
        tick += 1
        if tick % Self.renewEvery == 0 { await renewSignIn(lease) }
      }
    }
  }

  /// Renews the sign-in's lease with no picture; one the service no longer holds ends the sign-in here, saying why.
  ///
  /// - Parameter lease: The sign-in's lease.
  private func renewSignIn(_ lease: String) async {
    do {
      try await service.renewSignIn(lease: lease)
    } catch {
      guard signInLease == lease else { return }
      endSignIn()
      viewer?.signInEnded(error.localizedDescription)
    }
  }

  /// The sign-in ended at the service, by Done, Cancel or a lost lease: control ends here too, with nothing to give
  /// back.
  func endSignIn() {
    heartbeat?.cancel()
    heartbeat = nil
    letGoSignIn()
  }
}
