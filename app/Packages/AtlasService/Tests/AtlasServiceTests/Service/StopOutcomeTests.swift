import Foundation
import Testing

@testable import AtlasService

@Suite("Stop outcome")
struct StopOutcomeTests {
  /// The outcome `reply` decodes to.
  ///
  /// - Parameter reply: A Stop's reply, as the service writes it.
  /// - Returns: The decoded outcome.
  /// - Throws: When the reply is not a JSON object.
  private func outcome(_ reply: [String: Any]) throws -> StopOutcome {
    let data = try JSONSerialization.data(withJSONObject: reply)
    let decoder = JSONDecoder()
    return try decoder.decode(StopOutcome.self, from: data)
  }

  @Test("a Stop's reply names the jobs it could not end and what the service warned of")
  func aStopsReplyNamesWhatItCouldNotEnd() throws {
    let stopped = try outcome([
      "ok": true,
      "held": true,
      "warnings": ["Atlas could not stop 2 background jobs"],
      "jobs": [
        "stopping": 3,
        "ended": 1,
        "refused": [["id": "7c2e91a04b3f", "reason": "identity"]],
        "unknown": [["id": "aaaaaaaaaaaa"]],
        "unresolved": [["id": "bbbbbbbbbbbb"]],
      ],
    ])
    #expect(stopped.held)
    #expect(stopped.warnings == ["Atlas could not stop 2 background jobs"])
    #expect(stopped.refused == [StopOutcome.Job(id: "7c2e91a04b3f", reason: "identity")])
    #expect(stopped.unknown.map(\.id) == ["aaaaaaaaaaaa"])
    #expect(stopped.unresolved.map(\.id) == ["bbbbbbbbbbbb"])
    #expect(stopped.jobsLeft == 3)
    #expect(!stopped.isComplete)
  }

  @Test("a reply without jobs, or with odd entries, still counts every job it names")
  func oddRepliesKeepWhatTheyName() throws {
    let plain = try outcome(["ok": true, "held": true])
    #expect(plain.isComplete)
    #expect(plain.warnings.isEmpty)
    let odd = try outcome(["ok": true, "jobs": ["unresolved": [["id": 7], [:]], "refused": "none"]])
    #expect(odd.unresolved.count == 2, "a job with no readable ID is still a job left")
    #expect(odd.refused.isEmpty)
    #expect(!odd.isComplete)
  }
}
