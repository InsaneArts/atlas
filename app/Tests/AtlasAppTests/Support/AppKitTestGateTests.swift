import AsyncSupportTesting
import Testing

@Suite("AppKit test gate")
struct AppKitTestGateTests {
  @Test("another invocation cannot enter while the owner is suspended")
  func ownershipSurvivesSuspension() async throws {
    let gate = AppKitTestGate()
    let entered = ReleaseSignal()
    let release = ReleaseSignal()
    let attempted = ReleaseSignal()
    let calls = CallCount()
    let owner = Task {
      await #expect(throws: Never.self) {
        try await gate.withOwnership {
          await entered.open()
          await release.wait()
        }
      }
    }
    defer { release.open() }
    let arrival = Task { await entered.wait() }
    defer { arrival.cancel() }
    try await arrival.value("the owner never entered")
    let waiter = Task {
      attempted.open()
      await #expect(throws: Never.self) {
        try await gate.withOwnership {
          await MainActor.run { calls.count += 1 }
        }
      }
    }
    let attempt = Task { await attempted.wait() }
    defer { attempt.cancel() }
    try await attempt.value("the waiter never attempted entry")
    await afterQueuedWork()
    #expect(calls.count == 0, "the suspended owner still excludes the waiter")
    release.open()
    try await owner.value("the owner never finished")
    try await waiter.value("the waiter never inherited ownership")
    #expect(calls.count == 1)
  }

  @Test("a thrown failure releases ownership for the next invocation")
  func failureReleasesOwnership() async throws {
    let gate = AppKitTestGate()
    await #expect(throws: CancellationError.self) {
      try await gate.withOwnership { throw CancellationError() }
    }
    let calls = CallCount()
    let next = Task {
      await #expect(throws: Never.self) {
        try await gate.withOwnership {
          await MainActor.run { calls.count += 1 }
        }
      }
    }
    try await next.value("the failed invocation kept ownership")
    #expect(calls.count == 1)
  }
}
