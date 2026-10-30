import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ViewerStatus")
struct ViewerStatusTests {
  @Test("nonfinite status ages are read as unknown without dropping their rows")
  func nonfiniteAges() throws {
    let decoder = JSONDecoder()
    decoder.nonConformingFloatDecodingStrategy = .convertFromString(
      positiveInfinity: "Infinity",
      negativeInfinity: "-Infinity",
      nan: "NaN"
    )
    for age in ["Infinity", "-Infinity", "NaN"] {
      let data = Data(
        """
        {"overview":{"activity":[{"operation":"run","ok":true,"ageSeconds":"\(age)"}]},
        "capabilities":{"nativeDialogs":{"state":"passed","ageSeconds":"\(age)"}}}
        """.utf8
      )
      let reading = try decoder.decode(ViewerStatus.self, from: data)
      #expect(reading.overview.activity == [.init(operation: "run", ok: true)])
      #expect(reading.capabilities.nativeDialogs.state == .passed)
      #expect(reading.capabilities.nativeDialogs.ageSeconds == nil)
    }
  }

  @Test(
    "recovery checks each counter without overflow and reads negative counts as zero",
    arguments: [(Int.max, 1), (1, Int.max), (Int.max, Int.max), (Int.min, Int.min), (Int.min, 1), (1, Int.min), (0, 0)]
  )
  func recoveryCounts(pending: Int, inFlight: Int) throws {
    let reading = try status(["overview": ["pending": pending, "inFlight": inFlight]])
    #expect(reading.overview.pending == max(0, pending))
    #expect(reading.overview.inFlight == max(0, inFlight))
    #expect(reading.overview.recoveryRestarts == (pending > 0 || inFlight > 0))
  }

  @Test("an unknown outcome respects whether the service says recovery can help", arguments: [false, true])
  func recoveryEligibility(helps: Bool) throws {
    let reading = try status([
      "overview": ["mode": "recovering", "state": "needsYou", "pending": 1, "recoveryHelps": helps]
    ])
    #expect(reading.overview.mode == .recovering)
    #expect(reading.overview.pending == 1)
    #expect(reading.overview.recoveryHelps == helps)
  }

  @Test("a not ready status drops unsupported display sizes")
  func invalidDisplaySizes() throws {
    for side in [1e100, -1, 0, 32769] {
      for display in [["width": side, "height": 1], ["width": 1, "height": side]] {
        let reading = try status(["ready": false, "overview": ["display": display]])
        #expect(!reading.ready)
        #expect(reading.overview.display == nil)
      }
    }
    let decoder = JSONDecoder()
    decoder.nonConformingFloatDecodingStrategy = .convertFromString(
      positiveInfinity: "Infinity",
      negativeInfinity: "-Infinity",
      nan: "NaN"
    )
    for side in ["Infinity", "-Infinity", "NaN"] {
      let data = Data("{\"width\":\"\(side)\",\"height\":1}".utf8)
      #expect(throws: DecodingError.self) { try decoder.decode(ViewerStatus.Size.self, from: data) }
    }
  }

  @Test("the deleted accounts' open logins and a silent helper's login are read, and none by default")
  func leftoverLogins() throws {
    let reading = try status(["leftoverLogins": [502, 504], "helperStale": true])
    #expect(reading.leftoverLogins == [502, 504])
    #expect(reading.helperStale)
    let older = try status(["ready": true])
    #expect(older.leftoverLogins.isEmpty)
    #expect(!older.helperStale)
  }

  @Test("a status report decodes the session, the mode, the helper's names, its display and what it did")
  func decodes() throws {
    let reading = try status([
      "ready": true,
      "session": ["user": ["name": "atlastest"], "inputService": ["ok": true, "ready": true]],
      "overview": [
        "mode": "controlling", "state": "control", "label": "You have control", "recoverable": true,
        "account": "atlastest", "accountName": "Atlas Test",
        "display": ["width": 2560.0, "height": 1440.0], "pending": 0,
        "activity": [
          ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 3, "ageSeconds": 42.0],
          ["operation": "input", "ok": true, "says": "clicked Save in TextEdit"],
          ["operation": "run", "ok": true, "says": ""],
        ],
      ],
    ])
    #expect(reading.ready)
    #expect(reading.session.userName == "atlastest")
    #expect(reading.session.inputReady)
    #expect(reading.overview.mode == .controlling)
    #expect(reading.overview.state == .control)
    #expect(reading.overview.label == "You have control")
    #expect(reading.overview.recoverable)
    #expect(reading.overview.accountName == "Atlas Test")
    #expect(reading.overview.display == .init(width: 2560, height: 1440))
    #expect(
      reading.overview.activity == [
        .init(operation: "input", ok: true, actionCount: 3, actor: "viewer", ageSeconds: 42),
        .init(operation: "input", ok: true, says: "clicked Save in TextEdit"),
        .init(operation: "run", ok: true),
      ]
    )
  }

  @Test("Stop is offered while an agent acts or acted a minute ago, and Resume once stopped")
  func stopAndResume() throws {
    func overview(_ mode: String, agentAge: Double? = nil, actor: String = "agent") throws -> ViewerStatus.Overview {
      let rows = agentAge.map { [["operation": "input", "ok": true, "actor": actor, "ageSeconds": $0]] } ?? []
      return try status(["overview": ["mode": mode, "activity": rows]]).overview
    }
    #expect(try overview("agent").offersStop)
    #expect(try overview("observing", agentAge: 30).offersStop, "between two steps")
    #expect(try !overview("observing", agentAge: 90).offersStop)
    #expect(try !overview("observing", agentAge: 5, actor: "viewer").offersStop, "the person's own input")
    #expect(try !overview("stopped").offersStop)
    #expect(try overview("stopped").offersResume)
    #expect(try !overview("agent").offersResume)
  }

  @Test("Stop stays offered while a background job may run, however long ago an agent last acted")
  func stopStaysWhileJobsRun() throws {
    func overview(_ mode: String, running: Any?) throws -> ViewerStatus.Overview {
      let aged = [["operation": "job", "ok": true, "actor": "agent", "ageSeconds": 3600]]
      var fields: [String: Any] = ["mode": mode, "activity": aged]
      if let running {
        fields["jobs"] = ["running": running]
      }
      return try status(["overview": fields]).overview
    }
    #expect(try overview("observing", running: 1).offersStop, "an hour-old job still runs")
    #expect(try overview("observing", running: 1).jobs.running == 1)
    #expect(try !overview("observing", running: 0).offersStop)
    #expect(try !overview("observing", running: nil).offersStop, "no count reads as none")
    #expect(try !overview("observing", running: "two").offersStop, "a mistyped count reads as none")
    #expect(try overview("blocked", running: 2).offersStop)
    #expect(try !overview("stopped", running: 1).offersStop, "already stopped: Resume takes its place")
    #expect(try overview("controlling", running: 1).offersStop, "control ends no job, so Stop stays")
    #expect(try !overview("controlling", running: 0).offersStop, "no job and the person has control")
    #expect(try !overview("unavailable", running: 1).offersStop, "the helper is not reached")
  }

  @Test("a job still starting, or one whose record the helper cannot read, keeps Stop offered too")
  func stopStaysWhileJobsStartOrCannotBeRead() throws {
    func overview(_ jobs: [String: Any]) throws -> ViewerStatus.Overview {
      try status(["overview": ["mode": "observing", "jobs": jobs]]).overview
    }
    let launching = try overview(["launching": 1])
    #expect(launching.jobs.launching == 1)
    #expect(launching.offersStop)
    let unresolved = try overview(["running": 0, "unresolved": 2])
    #expect(unresolved.jobs.unresolved == 2)
    #expect(unresolved.offersStop, "a job Atlas cannot read may still run")
    #expect(try !overview(["launching": -1, "unresolved": "x"]).offersStop, "odd counts read as none")
  }

  @Test("missing or mistyped fields fall back to safe defaults, and an unknown mode reads as unavailable")
  func defaults() throws {
    let empty = try status([:])
    #expect(empty.overview.mode == .unavailable)
    #expect(empty.overview.state == .notReady)
    #expect(!empty.overview.recoverable)
    #expect(!empty.ready)
    #expect(empty.overview.activity.isEmpty)
    let odd = try status(["ready": "yes", "overview": ["mode": 3, "activity": "none"]])
    #expect(!odd.ready)
    #expect(odd.overview.mode == .unavailable)
    #expect(odd.overview.activity.isEmpty)
    let unknown = try status(["overview": ["mode": "dreaming"]])
    #expect(unknown.overview.mode == .unavailable, "a mode this app does not know reads as unavailable")
  }

  @Test("a failed native-dialog check names the first stage that did not pass")
  func nativeDialogStage() throws {
    let reading = try status([
      "capabilities": [
        "nativeDialogs": [
          "state": "failed", "reason": "No panel",
          "checks": [["stage": "open", "passed": true], ["stage": "save", "passed": false]],
        ]
      ]
    ])
    #expect(reading.capabilities.nativeDialogs.state == .failed)
    #expect(reading.capabilities.nativeDialogs.failedAt == "save")
  }
}
