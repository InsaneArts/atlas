import AtlasKit

extension ViewerController: InputSink {
  /// The least time between the readings input asks for: 200 ms. Each batch sent asks to read again, and a reading
  /// is a fresh capture and a PNG, so a moving mouse would otherwise keep them running back to back; five a second
  /// still shows what the input did.
  static let inputReadEvery: Duration = .milliseconds(200)

  /// Sends queued input to the service, counted as control work under way (``ControlBarrier``) until it answers: the
  /// person's own batch still in flight after Give Back holds the helper, so Take Control waits for it rather than
  /// meeting the service's busy answer and blaming an agent.
  ///
  /// - Parameters:
  ///   - lease: The lease the input belongs to.
  ///   - frame: The picture it was aimed at.
  ///   - actions: The actions, in order.
  /// - Throws: The service's error when it refuses the input or does not answer.
  func send(lease: String, frame: String, actions: [InputAction]) async throws {
    try await barrier.tracking { try await service.input(lease: lease, frame: frame, actions: actions) }
  }

  /// Input went: read again, to show its effect. At once after a quiet spell; while input keeps coming, as a moving
  /// mouse sends it, at most once each ``inputReadEvery``.
  func inputSent() {
    guard inputReading == nil else { return }
    let now = clock()
    let wait = inputReadAt.map { Self.inputReadEvery - .seconds(now - $0) } ?? .zero
    guard wait > .zero else {
      inputReadAt = now
      return refresh()
    }
    inputReading = Task { [weak self] in
      try? await Task.sleep(for: wait)
      guard let self, !Task.isCancelled else { return }
      inputReading = nil
      inputReadAt = clock()
      refresh()
    }
  }

  /// Input failed: control ends, saying why.
  ///
  /// - Parameter error: Why.
  func inputFailed(_ error: any Error) { releaseControl(error.localizedDescription) }
}
