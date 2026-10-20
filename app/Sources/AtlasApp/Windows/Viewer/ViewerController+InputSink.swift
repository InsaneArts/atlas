import AtlasKit

extension ViewerController: InputSink {
  /// Sends queued input to the service.
  ///
  /// - Parameters:
  ///   - lease: The lease the input belongs to.
  ///   - frame: The picture it was aimed at.
  ///   - actions: The actions, in order.
  /// - Throws: The service's error when it refuses the input or does not answer.
  func send(lease: String, frame: String, actions: [InputAction]) async throws {
    try await service.input(lease: lease, frame: frame, actions: actions)
  }

  /// Input went: read again soon, to show its effect.
  func inputSent() { refresh() }

  /// Input failed: control ends, saying why.
  ///
  /// - Parameter error: Why.
  func inputFailed(_ error: any Error) { releaseControl(error.localizedDescription) }
}
