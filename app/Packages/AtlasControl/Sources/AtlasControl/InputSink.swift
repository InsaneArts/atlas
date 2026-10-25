import AtlasService

/// Where the queue's input goes, and who hears how it went.
@MainActor
public protocol InputSink: AnyObject {
  /// Sends one batch under a lease.
  ///
  /// - Parameters:
  ///   - lease: The lease the batch belongs to.
  ///   - frame: The id of the picture its coordinates refer to.
  ///   - actions: The batch, in order.
  /// - Throws: Any error, which ends control.
  func send(lease: String, frame: String, actions: [InputAction]) async throws
  /// A batch was sent.
  func inputSent()
  /// A batch failed; control should end.
  ///
  /// - Parameter error: Why it failed.
  func inputFailed(_ error: any Error)
}
