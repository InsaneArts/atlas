import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityLog.Outcome")
struct ActivityLogOutcomeTests {
  @Test("logical identities decode leniently and fall back to the row id")
  func logicalIdentitiesFallBackToTheRowId() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "start", "logicalId": "command"},
        {"id": "older"},
        {"id": "empty", "logicalId": ""},
        {"id": "null", "logicalId": null},
        {"id": "malformed", "logicalId": 42}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.logicalId) == ["command", "older", "empty", "null", "malformed"])
    #expect(ActivityFixtures.row(id: "constructed").logicalId == "constructed")
  }

  @Test("an input refused part-way through is a failure, not refused before anything was sent")
  func partialRefusalIsAFailure() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "partial", "ok": false, "nothingSent": true, "partial": true},
        {"id": "whole", "ok": false, "nothingSent": true, "partial": false},
        {"id": "older", "ok": false, "nothingSent": true},
        {"id": "malformed", "ok": false, "nothingSent": true, "partial": "true"}
      ]}
      """#.utf8
    )
    let log = try ViewerClient.activity(body, response: reply(200))
    #expect(log.entries.map(\.outcome) == [.failed, .notSent, .notSent, .notSent])
    let filter = ActivityFilter.problems
    let problems = log.entries
      .filter { filter.admits($0) }
      .map(\.id)
    #expect(problems == ["partial"])
  }

  @Test("the shared service fixture preserves logical identities and reports partial focus refusals as problems")
  func partialServiceFixtureIsAProblem() throws {
    // The producer asserts this same repository fixture; keep the wire contract in one place.
    let repository = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
    let body = try Data(contentsOf: repository.appending(path: "tests/fixtures/activity-projection.json"))
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.count == 2)
    let entry = try #require(entries.first)
    #expect(entry.id == "405a9bc9382e3946")
    #expect(entry.logicalId == "c1")
    #expect(entry.outcome == .failed)
    #expect(ActivityFilter.problems.admits(entry))
    #expect(entry.reason == "Another window was in front")
    #expect(entries.last?.logicalId == "18919a007a04e825")
  }

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
