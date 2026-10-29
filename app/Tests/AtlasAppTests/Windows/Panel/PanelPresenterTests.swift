import AtlasServiceTesting
import AtlasUI
import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("PanelPresenter")
struct PanelPresenterTests {
  /// What the controller holds for one of the status table's rows, by its word: the service's mode and state for it,
  /// whether Atlas's main window holds control, and whether the panel is still connecting.
  ///
  /// - Parameter name: The row's word, such as "Needs you".
  /// - Returns: The snapshot.
  /// - Throws: A decoding error, which a fixed reply never gives, or when the row is not one of the table's.
  private static func snapshot(_ name: String) throws -> PanelSnapshot {
    let row = try #require(facts[name], "\(name) is not a row of the status table")
    return PanelSnapshot(
      status: try status(["overview": ["mode": row.mode, "state": row.state]]),
      connecting: name == "Connecting",
      holdingControl: row.holding
    )
  }

  /// Every row of the status table, by its word.
  private static let rows = [
    "Idle", "Working", "You have control", "Needs you", "Stopped", "Logged out", "Not ready", "Connecting",
    "In use elsewhere",
  ]

  /// Each row's facts: the service's mode and state for it, and whether Atlas's main window holds control.
  private static let facts: [String: (mode: String, state: String, holding: Bool)] = [
    "Idle": ("observing", "idle", false),
    "Working": ("agent", "working", false),
    "You have control": ("controlling", "control", true),
    "Needs you": ("recovering", "needsYou", false),
    "Stopped": ("stopped", "stopped", false),
    "Logged out": ("loggedOut", "notReady", false),
    "Not ready": ("unavailable", "notReady", false),
    "Connecting": ("observing", "idle", false),
    "In use elsewhere": ("controlling", "control", false),
  ]

  @Test(
    "each state offers its one main action, never greyed: Open Atlas while connecting, and Open Settings where Recover can't help",
    arguments: [
      ("Idle", "Open Atlas", PanelEvent.openAtlas),
      ("Working", "Open Atlas", .openAtlas),
      ("You have control", "Give Back", .giveBack),
      ("Needs you", "Recover…", .askRecover),
      ("Stopped", "Open Atlas", .openAtlas),
      ("Logged out", "Switch to the Helper…", .askSwitch),
      ("Not ready", "Open Settings", .openSettings),
      ("Connecting", "Open Atlas", .openAtlas),
      ("In use elsewhere", "Open Atlas", .openAtlas),
    ]
  )
  func eachStateOffersItsMainAction(row: String, title: String, event: PanelEvent) throws {
    let panel = PanelPresenter.state(try Self.snapshot(row))
    #expect(panel.status.label == row, "the facts read as \(row)")
    #expect(panel.controls.main.title == title)
    #expect(panel.controls.main.event == event)
    #expect(panel.controls.main.isEnabled)
  }

  @Test("Recover… opens its alert while a recovery runs, which shows it running; Switch… waits while a switch runs")
  func theMainActionWaitsOnlyForASwitch() throws {
    var needsYou = try Self.snapshot("Needs you")
    needsYou.running = [.recovery]
    #expect(PanelPresenter.state(needsYou).controls.main.isEnabled)
    var loggedOut = try Self.snapshot("Logged out")
    loggedOut.running = [.switchToHelper]
    #expect(!PanelPresenter.state(loggedOut).controls.main.isEnabled)
  }

  @Test("stopped offers Resume in glass before Open Atlas, the pair working offers with Stop")
  func stoppedPairsResumeWithOpenAtlas() throws {
    let stopped = PanelPresenter.state(try Self.snapshot("Stopped")).controls
    #expect(stopped.agent == .resume(isEnabled: true))
    #expect(stopped.main.event == .openAtlas)
    var resuming = try Self.snapshot("Stopped")
    resuming.agentRequests = 1
    #expect(PanelPresenter.state(resuming).controls.agent == .resume(isEnabled: false), "Resume waits for an answer")
  }

  @Test(
    "the picture wears the rim in control, greys under the badge while the helper needs the person, greys alone while paused, breathes while connecting, and gives way to why while logged out or not ready, with Recent"
  )
  func thePictureFollowsTheState() throws {
    func picture(_ row: String, paused: Bool = false) throws -> (PanelPictureState.Look, Bool) {
      var snapshot = try Self.snapshot(row)
      snapshot.session.paused = paused
      let state = PanelPresenter.state(snapshot)
      return (state.picture.look, state.recent.isEmpty)
    }
    #expect(try picture("You have control", paused: true).0 == .control)
    #expect(try picture("Needs you", paused: true).0 == .needsYou)
    #expect(try picture("Idle", paused: true).0 == .paused)
    #expect(try picture("Stopped").0 == .live)
    #expect(try picture("Connecting").0 == .connecting)
    #expect(try picture("Logged out") == (.empty(message: PanelWords.loggedOutPicture), true))
    #expect(
      try picture("Not ready") == (.empty(message: PanelWords.notReadyWaitPicture), true),
      "Recover cannot fix it"
    )
  }

  @Test("the panel's picture shows the control rim only while you have control")
  func theRimShowsOnlyInControl() throws {
    for row in Self.rows {
      for paused in [false, true] {
        var snapshot = try Self.snapshot(row)
        snapshot.session.paused = paused
        #expect(PanelPresenter.state(snapshot).picture.showsRim == (row == "You have control"), "\(row)")
      }
    }
  }

  @Test("needs you desaturates the picture and adds the badge, and paused dims it without one")
  func needsYouAndPausedDim() throws {
    for row in Self.rows {
      for paused in [false, true] {
        var snapshot = try Self.snapshot(row)
        snapshot.session.paused = paused
        let picture = PanelPresenter.state(snapshot).picture
        let livePicture = ["Idle", "Working", "Stopped", "In use elsewhere"].contains(row)
        #expect(picture.isDimmed == (row == "Needs you" || (paused && livePicture)), "\(row), paused \(paused)")
        #expect(picture.showsBadge == (row == "Needs you"), "\(row), paused \(paused)")
        #expect(picture.pause.isPaused == paused, "the pause button follows the picture alone")
      }
    }
  }

  @Test("only the newest row of Recent is live, and only while an action runs")
  func onlyTheNewestRowIsLive() throws {
    let activity: [[String: Any]] = [
      ["operation": "input", "actor": "agent", "ok": true, "ageSeconds": 2, "says": "clicked Save"],
      ["operation": "input", "actor": "agent", "ok": false, "ageSeconds": 9, "says": "opened TextEdit"],
      ["operation": "input", "actor": "agent", "ok": true, "ageSeconds": 31, "says": "opened Opie"],
    ]
    for row in Self.rows {
      let facts = try #require(Self.facts[row])
      let snapshot = PanelSnapshot(
        status: try status(["overview": ["mode": facts.mode, "state": facts.state, "activity": activity]]),
        connecting: row == "Connecting",
        holdingControl: facts.holding
      )
      let recent = PanelPresenter.state(snapshot).recent
      #expect(recent.map(\.isLive) == (recent.isEmpty ? [] : [row == "Working", false, false]), "\(row)")
      #expect(recent.map(\.isFailed) == (recent.isEmpty ? [] : [false, true, false]), "\(row)")
    }
  }

  @Test("the bottom row shows Stop only while the agent works, and Resume only once agents are stopped")
  func stopShowsOnlyWhileWorking() throws {
    for row in Self.rows {
      let agent = PanelPresenter.state(try Self.snapshot(row)).controls.agent
      let expected: ViewerAgentButton? =
        switch row {
        case "Working": .stop
        case "Stopped": .resume(isEnabled: true)
        default: nil
        }
      #expect(agent == expected, "\(row)")
    }
    var stopping = try Self.snapshot("Working")
    stopping.agentRequests = 1
    #expect(PanelPresenter.state(stopping).controls.agent == .stop, "a Stop under way leaves Stop in place")
  }

  @Test("an idle helper whose background job still runs keeps Stop in the bottom row")
  func stopStaysWhileABackgroundJobRuns() throws {
    let aged = [["operation": "job", "ok": true, "actor": "agent", "ageSeconds": 900]]
    let fields: [String: Any] = ["mode": "observing", "state": "idle", "activity": aged, "jobs": ["running": 1]]
    let snapshot = PanelSnapshot(status: try status(["overview": fields]), connecting: false, holdingControl: false)
    #expect(PanelPresenter.state(snapshot).controls.agent == .stop)
  }

  @Test("connecting says the helper is starting the first time, and reconnecting once answers came and stopped")
  func connectingSaysWhichKind() throws {
    var snapshot = try Self.snapshot("Connecting")
    #expect(PanelPresenter.state(snapshot).status.line == "Starting the helper…")
    snapshot.reconnecting = true
    #expect(PanelPresenter.state(snapshot).status.line == "Reconnecting…")
  }

  @Test("an idle helper with the picture paused says so, and that work in the helper goes on")
  func pausedIdleSaysSo() throws {
    var snapshot = try Self.snapshot("Idle")
    snapshot.session.paused = true
    #expect(PanelPresenter.state(snapshot).status.line == "Picture paused · work in the helper goes on")
    var working = try Self.snapshot("Working")
    working.session.paused = true
    #expect(PanelPresenter.state(working).status.line != PanelWords.pausedLine, "Working still says what it does")
  }

  @Test("while an agent works the picture carries its newest action alone, and none once paused")
  func theCaptionIsTheNewestActionAlone() throws {
    var snapshot = try Self.snapshot("Working")
    snapshot.status.overview.activity = [
      .init(operation: "input", ok: true, ageSeconds: 2, says: "clicked Save"),
      .init(operation: "input", ok: true, ageSeconds: 9, says: "opened TextEdit"),
    ]
    #expect(PanelPresenter.state(snapshot).picture.caption?.text == "Clicked Save")
    snapshot.session.paused = true
    #expect(PanelPresenter.state(snapshot).picture.caption == nil)
  }

  @Test("while an agent works a newest action that failed keeps its failure on the picture's caption")
  func aFailedNewestActionKeepsItsFailure() throws {
    var snapshot = try Self.snapshot("Working")
    snapshot.status.overview.activity = [
      .init(operation: "input", ok: false, ageSeconds: 2, says: "clicked Save"),
      .init(operation: "input", ok: true, ageSeconds: 9, says: "opened TextEdit"),
    ]
    #expect(PanelPresenter.state(snapshot).picture.caption?.isFailed == true)
    snapshot.status.overview.activity[0].ok = true
    #expect(PanelPresenter.state(snapshot).picture.caption?.isFailed == false)
  }

  @Test(
    "Recover… is the main action only where it fixes something, by the main window's rule: an unknown outcome or input it starts"
  )
  func recoverIsOfferedOnlyWhereItHelps() throws {
    func main(_ snapshot: PanelSnapshot) -> PanelEvent {
      PanelPresenter.state(snapshot).controls.main.event
    }
    #expect(main(try Self.snapshot("Needs you")) == .askRecover)
    #expect(main(try Self.snapshot("Not ready")) == .openSettings, "an unreachable service")
    let blocked = { (recoverable: Bool) throws -> PanelSnapshot in
      PanelSnapshot(
        status: try status([
          "overview": ["mode": "blocked", "state": recoverable ? "needsYou" : "notReady", "recoverable": recoverable]
        ])
      )
    }
    #expect(main(try blocked(true)) == .askRecover, "input Recover starts again")
    #expect(main(try blocked(false)) == .openSettings, "a block Recover can't fix")
  }

  @Test("while an agent's command runs, Needs you offers Open Atlas and says recovery waits, never Recover…")
  func recoveryWaitsForARunningCommand() throws {
    let snapshot = PanelSnapshot(
      status: try status([
        "overview": ["mode": "recovering", "state": "needsYou", "pending": 1, "commands": 1, "recoveryHelps": true]
      ])
    )
    let panel = PanelPresenter.state(snapshot)
    #expect(panel.controls.main.event == .openAtlas)
    #expect(panel.status.line == Words.unfinishedBesideCommandLine)
  }

  @Test("holding control while an agent's command runs says the command still runs, never that the agent waits")
  func controlBesideARunningCommand() throws {
    let snapshot = PanelSnapshot(
      status: try status(["overview": ["mode": "controlling", "state": "control", "commands": 1]]),
      holdingControl: true
    )
    let panel = PanelPresenter.state(snapshot)
    #expect(panel.status.label == "You have control")
    #expect(panel.status.line == Words.controlBesideCommandLine)
  }

  @Test("the ⋯ menu offers Open Atlas only while the main button is something else")
  func openAtlasIsNeverOfferedTwice() throws {
    for row in Self.rows {
      let snapshot = try Self.snapshot(row)
      let mainOpens = PanelPresenter.state(snapshot).controls.main.event == .openAtlas
      #expect(PanelPresenter.moreMenu(snapshot).showsOpenAtlas == !mainOpens, "\(row)")
    }
  }
}
