import Testing

@testable import AtlasService

@Suite("Frame storage budget")
struct FrameStorageBudgetTests {
  @Test("reservations are all or nothing and come back once")
  func reservations() async throws {
    let budget = FrameStorageBudget(limit: 100, allocations: 3)
    let first = try await budget.reserve(bytes: 60)
    await #expect(throws: FramePipeError.budgetExhausted) { try await budget.reserve(bytes: 41) }
    #expect(await budget.reserved == 60, "nothing is partly held")
    let second = try await budget.reserve(bytes: 40)
    #expect(first != second)
    await budget.release(first)
    await budget.release(first)
    #expect(await budget.reserved == 40, "a second release does nothing")
    await #expect(throws: FramePipeError.budgetExhausted) { try await budget.reserve(bytes: UInt64.max) }
  }

  @Test("the allocation count is bounded whatever the bytes")
  func allocations() async throws {
    let budget = FrameStorageBudget(limit: 1_000, allocations: 3)
    for _ in 0..<3 {
      _ = try await budget.reserve(bytes: 1)
    }
    await #expect(throws: FramePipeError.budgetExhausted) { try await budget.reserve(bytes: 1) }
  }
}
