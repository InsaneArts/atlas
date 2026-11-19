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
    #expect(stopped.refused == [StopOutcome.Job(id: "7c2e91a04b3f", reason: .identity)])
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

  @Test("a job that is not an object, or a warning that is not a string, drops nothing else")
  func aJobThatIsNotAnObjectStillCounts() throws {
    let odd = try outcome([
      "ok": true,
      "warnings": ["Atlas could not reach the helper", 7],
      "jobs": ["refused": ["7c2e91a04b3f", ["id": "aaaaaaaaaaaa", "reason": "leader-gone"]]],
    ])
    #expect(odd.warnings == ["Atlas could not reach the helper"])
    #expect(odd.refused == [StopOutcome.Job(id: ""), StopOutcome.Job(id: "aaaaaaaaaaaa", reason: .leaderGone)])
    #expect(odd.jobsLeft == 2)
  }

  /// A Stop's reply for Second Login and the VM `work`, in `order`: the VM's guest did not confirm it, and Second
  /// Login could not end one background job. Its top says what the first desktop said, as the service writes it.
  ///
  /// - Parameter order: The desktops' names, in the service's order.
  /// - Returns: The reply.
  private func everyDesktop(_ order: [String]) -> [String: Any] {
    let main: [String: Any] = [
      "desktop": "main",
      "uncertain": false,
      "reply": [
        "ok": true, "held": true, "warnings": ["Atlas could not stop 1 background job"],
        "jobs": ["refused": [["id": "7c2e91a04b3f", "reason": "identity"]]],
      ],
    ]
    let work: [String: Any] = [
      "desktop": "work",
      "uncertain": true,
      "reply": [
        "ok": true, "held": true, "delivered": false,
        "warnings": ["work has not confirmed this Stop. Agents are stopped here all the same."],
      ],
    ]
    let each = order.map { $0 == "main" ? main : work }
    var reply = (each.first?["reply"] as? [String: Any]) ?? [:]
    reply["desktops"] = each
    return reply
  }

  @Test("a Stop of every desktop reads what each said and names the uncertain one, in either order")
  func aStopOfEveryDesktopNamesTheUncertainOne() throws {
    for order in [["main", "work"], ["work", "main"]] {
      let stopped = try outcome(everyDesktop(order))
      #expect(stopped.held, "\(order)")
      #expect(stopped.uncertain == ["work"], "\(order)")
      #expect(stopped.stopped == ["main"], "\(order)")
      #expect(stopped.failed.isEmpty, "\(order)")
      #expect(stopped.refused.map(\.id) == ["7c2e91a04b3f"], "\(order)")
      #expect(stopped.jobsLeft == 1, "\(order)")
      #expect(stopped.warnings.count == 2, "\(order)")
      #expect(
        stopped.warnings.contains("work has not confirmed this Stop. Agents are stopped here all the same."),
        "\(order)"
      )
    }
  }

  @Test("a desktop whose Stop failed reads as not held and is named")
  func aFailedDesktopIsNotHeld() throws {
    let stopped = try outcome([
      "ok": true,
      "held": true,
      "desktops": [
        ["desktop": "main", "uncertain": false, "reply": ["ok": true, "held": true]],
        [
          "desktop": "work", "uncertain": true,
          "reply": ["ok": false, "code": "io", "error": "work could not be told"],
        ],
      ],
    ])
    #expect(!stopped.held)
    #expect(stopped.uncertain == ["work"])
    #expect(stopped.stopped == ["main"])
    #expect(stopped.failed == ["work"])
    #expect(stopped.warnings == ["work could not be told"])
  }

  @Test("with one desktop, a Stop names none uncertain")
  func oneDesktopNamesNone() throws {
    let stopped = try outcome(["ok": true, "held": true, "delivered": false])
    #expect(stopped.uncertain.isEmpty)
    #expect(stopped.stopped.isEmpty)
    #expect(stopped.failed.isEmpty)
  }
}
