/// An asynchronous FIFO gate: waiting tests suspend so AppKit and the owning test can keep using the main actor.
@MainActor
final class AppKitTestGate {
  /// The one gate shared by every AppKit suite in this test process.
  static let shared = AppKitTestGate()
  /// Whether an invocation owns the gate, even while it is suspended.
  private var owned = false
  /// Invocations waiting for their turn, oldest first.
  private var waiting: [CheckedContinuation<Void, Never>] = []

  /// Runs one invocation with exclusive ownership, handing it on even when the invocation throws.
  ///
  /// - Parameter function: The invocation and its deferred cleanup.
  /// - Throws: The invocation's failure.
  func withOwnership(_ function: @concurrent @Sendable () async throws -> Void) async throws {
    if owned {
      await withCheckedContinuation { waiting.append($0) }
    } else {
      owned = true
    }
    defer {
      if waiting.isEmpty {
        owned = false
      } else {
        waiting.removeFirst()
          .resume()
      }
    }
    try await function()
  }
}
