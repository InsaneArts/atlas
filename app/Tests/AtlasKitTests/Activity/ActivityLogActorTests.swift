import Foundation
import Testing

@testable import AtlasKit

@Suite("ActivityLog.Actor")
struct ActivityLogActorTests {
  @Test(
    "an activity row with an unknown actor reads as someone else",
    arguments: [
      ("cli", ActivityLog.Actor.someoneElse),
      // The two the service names, so a known word never falls through to someone else.
      ("viewer", .you),
      ("agent", .agent),
    ]
  )
  func unknownActor(word: String, actor: ActivityLog.Actor) throws {
    let log = try ViewerClient.activity(activity(actor: word), response: reply(200))
    #expect(log.entries.map(\.actor) == [actor])
  }
}
