import Foundation
import Testing
import os

@testable import AtlasService

@Suite("ServiceFollowing")
struct ServiceFollowingTests {
  /// A feed that gives `answers` in turn, then waits until cancelled; the positions it was asked from.
  final class ScriptedFeed: Sendable {
    /// The answers left, and the positions asked from.
    let state: OSAllocatedUnfairLock<(answers: [FeedAnswer], asked: [FeedPosition?])>

    init(_ answers: [FeedAnswer]) {
      state = OSAllocatedUnfairLock(initialState: (answers, []))
    }

    /// One wait on the feed.
    func ask(_ position: FeedPosition?) async throws(ServiceError) -> FeedAnswer {
      let next = state.withLock { state -> FeedAnswer? in
        state.asked.append(position)
        return state.answers.isEmpty ? nil : state.answers.removeFirst()
      }
      if let next { return next }
      try? await Task.sleep(for: .seconds(60))
      throw ServiceError("ended")
    }

    /// Whether every answer was given and the next is waited for.
    var done: Bool {
      state.withLock { $0.answers.isEmpty && !$0.asked.isEmpty }
    }
  }

  @Test("a reset reads everything again and the reader goes on from each event's position")
  func aResetThenEvents() async throws {
    let changes = ServiceChanges()
    let feed = ScriptedFeed([
      .reset(FeedPosition(feed: "f", after: 3)),
      .events(kinds: [.asks], last: FeedPosition(feed: "f", after: 4), poll: false),
      .events(kinds: [], last: nil, poll: false),
    ])
    let following = Task {
      await ServiceFollowing.follow(feed.ask, into: changes, clock: ContinuousClock())
    }
    while !feed.done || feed.state.withLock({ $0.asked.count }) < 4 { await Task.yield() }
    following.cancel()
    await following.value

    let asked = feed.state.withLock(\.asked)
    let third = FeedPosition(feed: "f", after: 3)
    let fourth = FeedPosition(feed: "f", after: 4)
    #expect(asked == [nil, third, fourth, fourth])
    #expect(changes.count(of: [.status]) == 1, "the reset")
    #expect(changes.count(of: [.asks]) == 2, "the reset and the event")
    #expect(!changes.followed, "a reader that stopped following keeps its own pace")
  }

  @Test("an unavailable feed is not followed, and readers keep their own pace")
  func anUnavailableFeed() async {
    let changes = ServiceChanges()
    let feed = ScriptedFeed([.reset(FeedPosition(feed: "f", after: 0)), .unavailable])
    let following = Task {
      await ServiceFollowing.follow(feed.ask, into: changes, clock: ContinuousClock())
    }
    while feed.state.withLock({ !$0.answers.isEmpty }) { await Task.yield() }
    while changes.followed { await Task.yield() }
    following.cancel()
    await following.value
    #expect(!changes.followed)
  }
}
