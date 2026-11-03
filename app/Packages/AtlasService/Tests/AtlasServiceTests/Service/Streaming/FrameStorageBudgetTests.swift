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
    await #expect(throws: FramePipeError.budgetExhausted, "the sum would overflow") {
      try await budget.reserve(bytes: UInt64.max)
    }
  }

  @Test("a reservation is released only by the budget that holds it")
  func foreignRelease() async throws {
    let window = FrameStorageBudget(limit: 100, allocations: 3)
    let other = FrameStorageBudget(limit: 100, allocations: 3)
    let held = try await window.reserve(bytes: 60)
    let foreign = try await other.reserve(bytes: 10)
    #expect(held.id == foreign.id, "both are their budget's first")
    await window.release(foreign)
    #expect(await window.reserved == 60)
    await other.release(held)
    #expect(await other.reserved == 10)
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
