import Foundation

/// Following the service's feed for as long as the app is attached to one start of it: each answer tells
/// ``ServiceChanges`` what to read again. A reset, or the feed found again, reads everything again. While the feed
/// cannot be followed (it is unavailable, every wait is taken, or it is not there), readers keep their own pace and
/// the feed is asked again a little later.
enum ServiceFollowing {
  /// How long the feed is left after every wait was taken.
  static let afterPoll: Duration = .seconds(2)
  /// How long a feed that could not be followed is left before it is asked again.
  static let probeEvery: Duration = .seconds(30)

  /// Follows the feed `ask` reads until the task is cancelled.
  ///
  /// - Parameters:
  ///   - ask: One wait on the feed, from a position.
  ///   - changes: Where what changed is told.
  ///   - clock: The clock pauses run on.
  static func follow(
    _ ask: @Sendable (FeedPosition?) async throws(ServiceError) -> FeedAnswer,
    into changes: ServiceChanges,
    clock: any Clock<Duration>
  ) async {
    var position: FeedPosition?
    while !Task.isCancelled {
      var pause: Duration = .zero
      do throws(ServiceError) {
        switch try await ask(position) {
        case .reset(let given):
          position = given
          changes.follow(true)
          changes.tell(Set(ChangeKind.allCases))
        case .events(let kinds, let last, let poll):
          position = last ?? position
          changes.follow(!poll)
          if !kinds.isEmpty { changes.tell(kinds) }
          if poll { pause = afterPoll }
        case .unavailable:
          changes.follow(false)
          pause = probeEvery
        }
      } catch {
        changes.follow(false)
        pause = probeEvery
      }
      if pause > .zero { try? await clock.sleep(for: pause) }
    }
    changes.follow(false)
  }
}
