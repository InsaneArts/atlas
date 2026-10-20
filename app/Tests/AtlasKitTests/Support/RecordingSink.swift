@testable import AtlasKit

/// An input sink that records each lease sent and each failure, and can hold
/// one lease's send open until the test resumes it.
@MainActor
final class RecordingSink: InputSink {
  var sent: [String] = []
  var failures: [String] = []
  var hold: CheckedContinuation<Void, any Error>?
  var holdLease: String?

  func send(lease: String, frame: String, actions: [InputAction]) async throws {
    sent.append(lease)
    if lease == holdLease {
      try await withCheckedThrowingContinuation { hold = $0 }
    }
  }
  func inputSent() {}
  func inputFailed(_ error: any Error) { failures.append(error.localizedDescription) }
}
