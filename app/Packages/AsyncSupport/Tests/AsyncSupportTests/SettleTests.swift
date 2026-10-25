import Testing

@testable import AsyncSupportTesting

@MainActor
@Suite("settle")
struct SettleTests {
  @Test("settle reads its condition again when an observed value it read changes")
  func settleEndsOnTheChange() async throws {
    let box = ObservedBox()
    var changing = false
    try await settle("the change was never seen") {
      // Changed once the wait has read the condition and begun to wait, so only the change can end it.
      if !changing {
        changing = true
        Task { @MainActor in box.value = 1 }
      }
      return box.value == 1
    }
  }

  @Test("a settle whose condition comes to hold with no change observed fails, saying it reads unobserved state")
  func settleFailsOnUnobservedState() async {
    let box = UnobservedBox()
    await withKnownIssue("a condition over unobserved state fails at the ceiling") {
      try await settle("the change was never seen", within: .milliseconds(50)) {
        // Read once as false, then true for good, with nothing observation sees changing.
        defer { box.value = 1 }
        return box.value == 1
      }
    }
  }
}
