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
    #expect(applied.steps.map(\.reason) == [.inUse, .changed, nil, nil])
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
    #expect(applied.steps.map(\.reason) == [nil, nil, nil, .otherVolume, nil])
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

  @Test("a check of collected files and a job's stop decode with what each left, counting every odd entry")
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
          "left": [{"pid": 4377, "name": "sleep"}, {"pid": "x"}, "not a process"]},
         {"n": 3, "do": "stop-job", "id": "8d3f02b15c4e", "name": "worker", "state": "failed",
          "reason": "still-stopping", "retry": true},
         {"n": 4, "do": "undo-plan", "state": "blocked", "reason": "output-not-kept"},
         {"n": 5, "do": "remove-copy", "state": "failed", "reason": "a-reason-from-later", "notKept": "none"}
       ]}
      """
    )
    #expect(applied.steps.map(\.kind) == [.checkOutputs, .stopJob, .stopJob, .undoPlan, .removeCopy])
    #expect(applied.steps[0].notKept == 3, "a file not kept counts, whatever its entry holds")
    #expect(applied.steps[1].job == "7c2e91a04b3f")
    #expect(applied.steps[1].name == "web")
    #expect(
      applied.steps[1].left == [CleanupProcess(name: "sleep"), CleanupProcess(name: "")],
      "a process with no name still counts, and an entry that is not one is dropped"
    )
    #expect(applied.steps[2].reason == .stillStopping && applied.steps[2].retry)
    #expect(applied.steps[3].left.isEmpty && applied.steps[3].notKept == 0)
    #expect(applied.steps[4].reason == .unknown && applied.steps[4].notKept == 0)
  }

  @Test("an apply's ending reads how it went, and a held-back one says why")
  func anApplysEndingReadsHowItWent() {
    #expect(CleanupReply(ok: true, state: .complete).ending == .nothingToCleanUp)
    let quit = CleanupStep(number: 1, kind: .quit, state: .done)
    #expect(CleanupReply(ok: true, state: .complete, steps: [quit]).ending == .cleanedUp)
    #expect(CleanupReply(ok: false, state: .incomplete, steps: [quit]).ending == .unfinished)
    #expect(CleanupReply(state: .uncertain, steps: [quit]).ending == .uncertain)
    let held = [
      CleanupStep(number: 1, kind: .stopJob, state: .blocked, reason: .daemon),
      CleanupStep(number: 2, kind: .removeCopy, state: .blocked, reason: .outputNotKept),
    ]
    #expect(CleanupReply(state: .blocked, steps: held).ending == .blocked(.outputNotKept))
    #expect(CleanupReply(state: .blocked, steps: [held[0]]).ending == .blocked(.daemon))
    let stopped = CleanupStep(number: 1, kind: .removeCopy, state: .blocked, reason: .stopped)
    #expect(CleanupReply(state: .blocked, steps: [stopped]).ending == .blocked(.stopped))
  }

  @Test("a cleanup whose reply never came ends by what its steps show once read again")
  func anUnansweredCleanupEndsByItsSteps() {
    #expect(CleanupReply(preview: true).unansweredEnding == .cleanedUp)
    let notRun = CleanupReply(preview: true, steps: [CleanupStep(number: 1, kind: .quit)])
    #expect(notRun.unansweredEnding == nil && !notRun.hasRun)
    let started = CleanupReply(preview: true, steps: [CleanupStep(number: 1, kind: .quit, state: .started)])
    #expect(started.unansweredEnding == .uncertain && started.hasRun)
    let failed = CleanupReply(preview: true, steps: [CleanupStep(number: 1, kind: .quit, state: .failed)])
    #expect(failed.unansweredEnding == .unfinished)
    let blocked = CleanupStep(number: 1, kind: .stopJob, state: .blocked, reason: .daemon)
    #expect(CleanupReply(preview: true, steps: [blocked]).unansweredEnding == .blocked(.daemon))
  }

  @Test("the steps still to do leave out those done or kept")
  func theOpenStepsLeaveOutThoseDoneOrKept() {
    let preview = CleanupReply(
      preview: true,
      steps: [
        CleanupStep(number: 1, kind: .quit, state: .done),
        CleanupStep(number: 2, kind: .removeInstall, state: .kept),
        CleanupStep(number: 3, kind: .removeCopy, state: .failed),
        CleanupStep(number: 4, kind: .undoPlan),
      ]
    )
    #expect(preview.openSteps.map(\.number) == [3, 4])
  }
}
