import Testing

@testable import AtlasOnboarding

@Suite("ReadingPace")
struct ReadingPaceTests {
  /// A pace whose first reading started at `start`, with no change since.
  private func started(at start: Duration = .zero) -> ReadingPace {
    var pace = ReadingPace()
    pace.start(at: start)
    return pace
  }

  @Test("a change reads at once, but never sooner than the gap after the last reading")
  func changeWaitsForTheGap() {
    var pace = started()
    pace.change(at: .milliseconds(200))
    #expect(pace.next == ReadingPace.minimumGap)
    pace.start(at: .seconds(5))
    pace.change(at: .seconds(14))
    // Past the close watch, a change still reads as soon as the gap allows.
    #expect(pace.next == .milliseconds(6500))
  }

  @Test("after a change the window reads at the gap for the close watch, then falls back")
  func closeWatch() {
    var pace = started()
    pace.change(at: .seconds(1))
    var starts: [Duration] = []
    for _ in 0..<12 {
      let next = pace.next
      starts.append(next)
      pace.start(at: next)
    }
    let watched = starts.filter { $0 < .seconds(11) }
    #expect(zip(watched, watched.dropFirst()).allSatisfy { $1 - $0 == ReadingPace.minimumGap })
    let after = starts.filter { $0 >= .seconds(11) }
    #expect(zip(after, after.dropFirst()).allSatisfy { $1 - $0 == ReadingPace.fallbackEvery })
    #expect(!after.isEmpty)
  }

  @Test("a deadline brings a reading forward, but never before the gap")
  func deadline() {
    var pace = started()
    pace.read(.agentsRetry, at: .seconds(4))
    #expect(pace.next == .seconds(4))
    pace.read(.settled, at: .milliseconds(100))
    #expect(pace.next == ReadingPace.minimumGap)
    pace.drop(.settled)
    #expect(pace.next == .seconds(4))
  }

  @Test("a deadline a reading has passed is dropped, and a later one stays")
  func passedDeadline() {
    var pace = started()
    pace.read(.agentsRetry, at: .seconds(4))
    pace.read(.restartGrace, at: .seconds(30))
    pace.start(at: .seconds(4))
    #expect(pace.next == .seconds(14))
    pace.start(at: .seconds(14))
    #expect(pace.next == .seconds(24))
    pace.start(at: .seconds(24))
    #expect(pace.next == .seconds(30))
  }

  @Test("asking again for a deadline moves it")
  func movedDeadline() {
    var pace = started()
    pace.read(.restartGrace, at: .seconds(3))
    pace.read(.restartGrace, at: .seconds(8))
    #expect(pace.next == .seconds(8))
  }

  @Test("the window's own ask reads as soon as the reading under way ends, and starts the close watch")
  func ask() {
    var pace = started()
    pace.ask(at: .milliseconds(300))
    #expect(pace.next == .zero, "at once")
    pace.start(at: .milliseconds(400))
    #expect(pace.next == .milliseconds(1900), "then at the gap, for the close watch")
  }
}
