import Foundation
import Testing

@testable import AtlasService

@Suite("ServiceChanges")
struct ServiceChangesTests {
  /// How long `wait` took on the wall clock.
  private func timed(_ wait: () async -> Void) async -> Duration {
    let clock = ContinuousClock()
    let start = clock.now
    await wait()
    return start.duration(to: clock.now)
  }

  @Test("a change told while a reader waits wakes it at once")
  func aChangeWakesAWaitingReader() async {
    let changes = ServiceChanges()
    let seen = changes.count(of: [.status])
    let waiting = Task { await changes.pause(.seconds(20), on: [.status], since: seen, clock: ContinuousClock()) }
    try? await Task.sleep(for: .milliseconds(50))
    let took = await timed {
      changes.tell([.status])
      await waiting.value
    }
    #expect(took < .seconds(5))
  }

  @Test("a change that came while the reader read is read again at once")
  func aChangeDuringAReadIsReadAgain() async {
    let changes = ServiceChanges()
    let seen = changes.count(of: [.activity])
    changes.tell([.activity])
    let took = await timed {
      await changes.pause(.seconds(20), on: [.activity], since: seen, clock: ContinuousClock())
    }
    #expect(took < .seconds(5))
  }

  @Test("another kind's change leaves the reader its own pause")
  func anotherKindLeavesThePause() async {
    let changes = ServiceChanges()
    let seen = changes.count(of: [.asks])
    let took = await timed {
      changes.tell([.status])
      await changes.pause(.milliseconds(200), on: [.asks], since: seen, clock: ContinuousClock())
    }
    #expect(took >= .milliseconds(200))
  }

  @Test("while the feed is followed a reader waits for it rather than its own pause")
  func aFollowedFeedStretchesThePause() async {
    let changes = ServiceChanges()
    changes.follow(true)
    let seen = changes.count(of: [.sessions])
    let took = await timed {
      let waiting = Task {
        await changes.pause(.milliseconds(50), on: [.sessions], since: seen, clock: ContinuousClock())
      }
      try? await Task.sleep(for: .milliseconds(300))
      changes.tell([.sessions])
      await waiting.value
    }
    #expect(took >= .milliseconds(300))
    #expect(took < .seconds(10))
  }

  @Test("a cancelled wait ends at once")
  func aCancelledWaitEnds() async {
    let changes = ServiceChanges()
    let took = await timed {
      let waiting = Task { await changes.pause(.seconds(20), on: [.control], since: 0, clock: ContinuousClock()) }
      waiting.cancel()
      await waiting.value
    }
    #expect(took < .seconds(5))
  }
}
