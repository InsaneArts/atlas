import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityLog.Outcome")
struct ActivityLogOutcomeTests {
  @Test(
    "a command under way reads as running though the service marks it ok",
    arguments: [
      (true, true, ActivityLog.Outcome.running),
      (false, true, .worked),
      (false, false, .failed),
    ]
  )
  func running(started: Bool, ok: Bool, outcome: ActivityLog.Outcome) throws {
    let log = try ViewerClient.activity(activity(ok: ok, started: started), response: reply(200))
    #expect(log.entries.map(\.outcome) == [outcome])
  }
}
