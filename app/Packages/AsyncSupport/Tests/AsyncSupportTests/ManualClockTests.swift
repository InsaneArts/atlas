import Testing

@testable import AsyncSupportTesting

@MainActor
@Suite("ManualClock")
struct ManualClockTests {
  @Test("a manual clock resumes sleepers in deadline order")
  func resumesSleepersInDeadlineOrder() async throws {
    let clock = ManualClock()
    // Begun one at a time, out of deadline order, two with one deadline: the earliest is resumed first, and of those
    // two the one that began first. Ids rise in the order sleepers begin.
    var sleepers: [Task<Void, Never>] = []
    for (began, seconds) in [3, 1, 2, 2].enumerated() {
      sleepers.append(Task { try? await clock.sleep(for: .seconds(seconds)) })
      try await clock.untilSleeping(began + 1, "sleeper \(began) never slept")
    }
    #expect(clock.waiting == [.seconds(1), .seconds(2), .seconds(2), .seconds(3)])
    clock.advance(by: .seconds(5))
    for sleeper in sleepers {
      try await sleeper.value("a sleeper never woke")
    }
    #expect(clock.woken == [1, 2, 3, 0])
    #expect(clock.registered == 0)
  }

  @Test("a manual clock resumes a sleeper whose deadline is already past at once")
  func resumesAPastDeadlineAtOnce() async throws {
    let clock = ManualClock()
    clock.advance(by: .seconds(5))
    try await clock.sleep(until: ManualClock.Instant(offset: .seconds(2)), tolerance: nil)
    try await clock.sleep(for: .zero)
    #expect(clock.registered == 0)
    #expect(clock.waiting.isEmpty)
  }

  @Test("a cancelled sleeper throws CancellationError and is forgotten, so advancing past it wakes nothing")
  func aCancelledSleeperIsForgotten() async throws {
    let clock = ManualClock()
    let sleeper = Task {
      do {
        try await clock.sleep(for: .seconds(10))
        return false
      } catch {
        return error is CancellationError
      }
    }
    try await clock.untilSleeping(for: .seconds(10), "the sleeper never slept")
    sleeper.cancel()
    #expect(try await sleeper.value("the cancelled sleeper never ended"))
    #expect(clock.registered == 0)
    // A forgotten sleeper is never resumed a second time, which a checked continuation would trap on.
    clock.advance(by: .seconds(10))
    #expect(clock.waiting.isEmpty)
  }

  @Test("each change publishes the sleepers once, in order, an advance that wakes nothing included")
  func eachChangePublishesOnce() async throws {
    let clock = ManualClock()
    let counts = clock.sleepers()
    // Unstructured, so the ceiling bounds the reading of the first five counts.
    let reading = Task {
      var seen: [Int] = []
      for await count in counts {
        seen.append(count)
        if seen.count == 5 { break }
      }
      return seen
    }
    let sleeper = Task { try? await clock.sleep(for: .seconds(10)) }
    try await clock.untilSleeping(for: .seconds(10), "the sleeper never slept")
    clock.advance(by: .seconds(9))
    clock.advance(by: .seconds(1))
    try await sleeper.value("the sleeper never woke")
    let next = Task { try? await clock.sleep(for: .seconds(1)) }
    // Now, the sleeper beginning, an advance short of it, the advance that wakes it, the next one beginning.
    #expect(try await reading.value("five counts never came") == [0, 1, 1, 0, 1])
    next.cancel()
    try await next.value("the last sleeper never ended")
  }

  @Test("a reader that stops is forgotten at once")
  func aStoppedReaderIsForgotten() async throws {
    let clock = ManualClock()
    let deadlines = clock.deadlines()
    // Unstructured, so the test stops it.
    let reading = Task {
      for await _ in deadlines {}
    }
    #expect(clock.observing == 1)
    reading.cancel()
    try await reading.value("the stopped reader never ended")
    #expect(clock.observing == 0)
  }

  @Test("a stream of the sleepers finishes when the clock goes away")
  func aStreamFinishesWithTheClock() async throws {
    // The clock goes away as soon as it has handed out its stream, which holds it only weakly.
    let counts = ManualClock()
      .sleepers()
    // Unstructured, so the ceiling bounds the wait for the stream's end.
    let reading = Task {
      var seen: [Int] = []
      for await count in counts {
        seen.append(count)
      }
      return seen
    }
    #expect(try await reading.value("the stream never finished") == [0])
  }
}
