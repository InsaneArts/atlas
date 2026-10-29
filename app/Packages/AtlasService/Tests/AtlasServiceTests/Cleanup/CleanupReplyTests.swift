import Foundation
import Testing

@testable import AtlasService

@Suite("CleanupReply")
struct CleanupReplyTests {
  /// A cleanup body decoded.
  ///
  /// - Parameter body: The body's JSON.
  /// - Returns: The reply.
  /// - Throws: A decoding error when the body is not an object.
  func decoded(_ body: String) throws -> CleanupReply {
    try JSONDecoder()
      .decode(CleanupReply.self, from: Data(body.utf8))
  }

  @Test("a reply without steps decodes as nothing to clean up")
  func aReplyWithoutStepsIsNothingToCleanUp() throws {
    let empty = try decoded(#"{"ok": true, "preview": false, "state": "complete", "steps": [], "receipt": null}"#)
    #expect(empty.isEmpty)
    #expect(empty.ok && empty.state == .complete)
    let bare = try decoded("{}")
    #expect(bare.isEmpty && !bare.ok && !bare.preview && !bare.active)
    #expect(bare.state == nil && bare.error == nil && bare.left.isEmpty)
  }

  @Test("an unknown step kind still decodes")
  func anUnknownStepKindStillDecodes() throws {
    let preview = try decoded(
      """
      {"ok": true, "preview": true, "active": true,
       "session": {"key": "3f9a1c07b2e4", "name": "Claude Code · left"},
       "steps": [
         {"n": 1, "do": "quit", "apps": [{"app": "Probe", "pid": 812}, {"pid": 813}]},
         {"n": 2, "do": "undo-plan", "planId": "p1", "planState": "applied", "targets": ["~/Library"]},
         {"n": 3, "do": "remove-install", "installId": "i1", "app": "Probe", "path": "/Applications/Probe.app",
          "present": true, "unchanged": false},
         {"n": 4, "do": "remove-copy", "path": "/tmp/copy", "app": null, "present": true},
         {"n": 5, "do": "reboot-the-moon", "app": "Moon"},
         "not a step"
       ],
       "left": [{"app": "TextEdit", "why": "opened by name"}, {"why": "no name"}]}
      """
    )
    #expect(preview.preview && preview.active)
    #expect(preview.steps.map(\.kind) == [.quit, .undoPlan, .removeInstall, .removeCopy, .unknown])
    #expect(preview.steps.map(\.number) == [1, 2, 3, 4, 5])
    #expect(preview.steps.first?.apps == ["Probe"])
    #expect(preview.steps[2].unchanged == false)
    #expect(preview.steps[3].app == nil)
    #expect(preview.steps.allSatisfy { $0.state == nil }, "a preview step that has not run has no state")
    #expect(preview.left == ["TextEdit"])
  }

  @Test("an apply's steps read how far each got, and why")
  func anApplysStepsReadHowFarEachGot() throws {
    let applied = try decoded(
      """
      {"ok": false, "preview": false, "state": "incomplete", "code": "cleanup",
       "error": "Cleanup is not finished: 1 step failed.",
       "steps": [
         {"n": 1, "do": "quit", "state": "failed", "reason": "in-use", "retry": true},
         {"n": 2, "do": "remove-install", "app": "Probe", "state": "kept", "reason": "changed"},
         {"n": 3, "do": "remove-copy", "app": "Probe", "state": "done", "alreadyGone": true},
         {"n": 4, "do": "remove-copy", "state": "sideways"}
       ]}
      """
    )
    #expect(applied.state == .incomplete && applied.error == "Cleanup is not finished: 1 step failed.")
    #expect(applied.steps.map(\.state) == [.failed, .kept, .done, .unknown])
    #expect(applied.steps.map(\.reason) == ["in-use", "changed", nil, nil])
    #expect(applied.steps.map(\.retry) == [true, false, false, false])
    #expect(applied.steps.map(\.alreadyGone) == [false, false, true, false])
    #expect(!applied.isUnderWay, "no step is under way once the apply replied")
  }

  @Test("a held step keeps where in the helper it was moved aside to")
  func aHeldStepKeepsWhereInTheHelperItWasMovedAsideTo() throws {
    let applied = try decoded(
      """
      {"ok": true, "preview": false, "state": "complete",
       "steps": [
         {"n": 1, "do": "remove-install", "app": "Probe", "state": "done",
          "held": "/Users/atlas/.config/atlas/installs/held/a7d2c0ffee12/Probe.app"},
         {"n": 2, "do": "remove-install", "app": "Opie", "state": "done", "alreadyGone": true},
         {"n": 3, "do": "remove-copy", "app": "Notes", "state": "done", "alreadyGone": true},
         {"n": 4, "do": "remove-install", "app": "Mail", "state": "kept", "reason": "other-volume"},
         {"n": 5, "do": "remove-install", "state": "done", "held": 7}
       ]}
      """
    )
    #expect(
      applied.steps.map(\.held) == [
        "/Users/atlas/.config/atlas/installs/held/a7d2c0ffee12/Probe.app", nil, nil, nil, nil,
      ]
    )
    #expect(applied.steps.map(\.alreadyGone) == [false, true, true, false, false])
    #expect(applied.steps.map(\.reason) == [nil, nil, nil, "other-volume", nil])
    #expect(applied.steps.map(\.state) == [.done, .done, .done, .kept, .done], "a path of the wrong type is left out")
  }

  @Test("a preview with a step started reads as under way")
  func aPreviewWithAStepStartedIsUnderWay() throws {
    let running = try decoded(
      """
      {"ok": true, "preview": true,
       "steps": [{"n": 1, "do": "quit", "state": "done"}, {"n": 2, "do": "remove-copy", "state": "started"}]}
      """
    )
    #expect(running.isUnderWay)
  }

  @Test("an apply that ran and did not finish comes back as its reply, while any other refusal is an error")
  func anUnfinishedApplyIsAReply() throws {
    let unfinished = Data(
      #"{"ok": false, "code": "cleanup", "state": "blocked", "steps": [{"n": 1, "do": "quit", "state": "blocked"}]}"#
        .utf8
    )
    let read = try ViewerClient.cleanup(unfinished, response: reply(409))
    #expect(!read.ok && read.state == .blocked && read.steps.count == 1)
    let busy = Data(#"{"ok": false, "code": "busy", "error": "Release control before cleaning up"}"#.utf8)
    #expect(throws: ServiceError("Release control before cleaning up", code: "busy")) {
      try ViewerClient.cleanup(busy, response: reply(409))
    }
    let unknown = Data(#"{"ok": false, "code": "input", "error": "No such session"}"#.utf8)
    #expect(throws: ServiceError("No such session", code: "input")) {
      try ViewerClient.cleanup(unknown, response: reply(400))
    }
    #expect(throws: ServiceError.notOffered) { try ViewerClient.cleanup(Data(), response: reply(404)) }
  }

  @Test("a preview asks for its session by key, and an apply posts it")
  func thePreviewAndTheApplyNameTheSession() throws {
    let client = try #require(ViewerClient("http://127.0.0.1:8765/abc/"))
    let preview = try client.cleanupPreviewRequest(session: "3f9a1c07b2e4")
    #expect(preview.url?.absoluteString == "http://127.0.0.1:8765/abc/cleanup.json?session=3f9a1c07b2e4")
    #expect(preview.httpMethod == "GET")
    let apply = try client.cleanupApplyRequest(session: "3f9a1c07b2e4")
    #expect(apply.url?.lastPathComponent == "cleanup" && apply.httpMethod == "POST")
    let body = try JSONSerialization.jsonObject(with: try #require(apply.httpBody)) as? [String: String]
    #expect(body == ["session": "3f9a1c07b2e4"])
  }

  @Test("a check of collected files and a job's stop decode with what each left")
  func aCheckOfCollectedFilesAndAJobsStopDecodeWithWhatEachLeft() throws {
    let applied = try decoded(
      """
      {"ok": false, "preview": false, "state": "blocked", "code": "cleanup",
       "steps": [
         {"n": 1, "do": "check-outputs", "receipts": ["1761650000123-a1b2c3", 7], "state": "failed",
          "reason": "output-not-kept",
          "notKept": [{"source": "/Users/atlas/atlas-work/report.pdf", "inHome": "atlas-work/report.pdf"},
                      {"source": "/private/tmp/out.png"}, {"inHome": "no source"}]},
         {"n": 2, "do": "stop-job", "id": "7c2e91a04b3f", "name": "web", "state": "done",
          "left": [{"pid": 4377, "name": "sleep"}, {"pid": "x"}]},
         {"n": 3, "do": "stop-job", "id": "8d3f02b15c4e", "name": "worker", "state": "failed",
          "reason": "still-stopping", "retry": true},
         {"n": 4, "do": "undo-plan", "state": "blocked", "reason": "output-not-kept"}
       ]}
      """
    )
    #expect(applied.steps.map(\.kind) == [.checkOutputs, .stopJob, .stopJob, .undoPlan])
    let check = applied.steps[0]
    #expect(check.receipts == ["1761650000123-a1b2c3"], "an entry that is not a receipt is dropped")
    #expect(
      check.notKept == [
        CleanupFile(source: "/Users/atlas/atlas-work/report.pdf", inHome: "atlas-work/report.pdf"),
        CleanupFile(source: "/private/tmp/out.png"),
      ]
    )
    #expect(applied.steps[1].job == "7c2e91a04b3f")
    #expect(applied.steps[1].name == "web")
    #expect(applied.steps[1].left == [CleanupProcess(pid: 4377, name: "sleep")])
    #expect(applied.steps[2].reason == "still-stopping" && applied.steps[2].retry)
    #expect(applied.steps[3].left.isEmpty && applied.steps[3].notKept.isEmpty)

    let preview = try decoded(
      """
      {"ok": true, "preview": true, "steps": [
         {"n": 1, "do": "check-outputs", "receipts": ["1761650000123-a1b2c3"], "intact": false},
         {"n": 2, "do": "stop-job", "id": "7c2e91a04b3f", "name": "web", "jobState": "running"}]}
      """
    )
    #expect(preview.steps[0].intact == false)
    #expect(preview.steps[1].jobState == "running")
  }
}
