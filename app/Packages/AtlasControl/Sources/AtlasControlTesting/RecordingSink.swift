import AtlasControl
import AtlasService
import Observation

/// An input sink that records each lease sent and each failure, and can hold
/// one lease's send open until the test resumes it.
@MainActor
@Observable
public final class RecordingSink: InputSink {
  /// The lease of each batch sent, in order.
  public var sent: [String] = []
  /// Each failure's message, in order.
  public var failures: [String] = []
  /// The held send, until the test resumes it.
  public var hold: CheckedContinuation<Void, any Error>?
  /// The lease whose send is held open.
  public var holdLease: String?

  /// A sink that has sent nothing and holds no lease.
  public init() {}

  /// Records the lease, and holds the send open when it is `holdLease`.
  ///
  /// - Parameters:
  ///   - lease: The lease the input belongs to.
  ///   - frame: The id of the picture the input's coordinates refer to.
  ///   - actions: The actions, in order.
  /// - Throws: What the test resumes `hold` with.
  public func send(lease: String, frame: String, actions: [InputAction]) async throws {
    sent.append(lease)
    if lease == holdLease {
      try await withCheckedThrowingContinuation { hold = $0 }
    }
  }

  /// Hears that a batch was sent, and records nothing.
  public func inputSent() {}

  /// Records the failure's message.
  ///
  /// - Parameter error: Why the input failed.
  public func inputFailed(_ error: any Error) { failures.append(error.localizedDescription) }
}
