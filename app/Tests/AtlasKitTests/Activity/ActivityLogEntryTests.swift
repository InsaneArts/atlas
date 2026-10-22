import Foundation
import Testing

@testable import AtlasKit

@Suite("ActivityLog.Entry")
struct ActivityLogEntryTests {
  @Test("an activity row with every field null reads as an agent's failed action")
  func allNull() throws {
    let log = try ViewerClient.activity(activityAllNull, response: reply(200))
    #expect(log.entries == [.init(id: "9f2c41d07a3b55e1", outcome: .failed, actor: .agent, operation: "action")])
  }

  @Test("an activity row's start is read as milliseconds since 1970")
  func startedAt() throws {
    let body = Data(#"{"activity": [{"id": "7", "startedAt": 1792665097600, "ageSeconds": 4}]}"#.utf8)
    let log = try ViewerClient.activity(body, response: reply(200))
    #expect(log.entries.first?.startedAt == Date(timeIntervalSince1970: 1_792_665_097.6))
  }
}
