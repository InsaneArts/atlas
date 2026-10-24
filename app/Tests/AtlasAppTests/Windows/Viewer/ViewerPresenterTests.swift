import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter")
struct ViewerPresenterTests {
  /// A viewer that has read `status` and a picture taken just now.
  ///
  /// - Parameter status: What the service reports.
  /// - Returns: The snapshot.
  func watching(_ status: ViewerStatus) -> ViewerSnapshot {
    var snapshot = ViewerSnapshot()
    snapshot.reading.status = status
    snapshot.reading.hasStatus = true
    snapshot.reading.ready = true
    snapshot.reading.frame = Frame(png: Data(), id: "f", revision: "r", display: ViewerStatus.Size(width: 4, height: 3))
    snapshot.reading.frameAt = 0
    return snapshot
  }

  @Test(
    "the status reads Connecting before the first reading and while the picture is lost, unless the helper's own state says more"
  )
  func theStatusReadsConnectingFirst() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    #expect(ViewerPresenter.state(snapshot).top.status.label == "Idle")
    snapshot.session.paused = true
    #expect(ViewerPresenter.state(snapshot).top.status.line == Words.pausedLine, "a paused picture is Idle's line")
    snapshot.reading.unavailable = "Gone"
    #expect(ViewerPresenter.state(snapshot).top.status.label == "Connecting")
    snapshot.reading.status = try status(["overview": ["mode": "stopped", "state": "stopped"]])
    #expect(ViewerPresenter.state(snapshot).top.status.label == "Stopped", "a stop the window knows of stays")
    #expect(ViewerPresenter.state(ViewerSnapshot()).top.status.label == "Connecting")
  }

  @Test("the account reads the full name, then the session's user, then the short name, then Helper")
  func theAccountFallsBackInOrder() throws {
    #expect(ViewerPresenter.account(try status(["overview": ["account": "atlas", "accountName": "Atlas"]])) == "Atlas")
    #expect(
      ViewerPresenter.account(try status(["session": ["user": ["name": "user"]], "overview": ["account": "atlas"]]))
        == "user"
    )
    #expect(ViewerPresenter.account(try status(["overview": ["account": "atlas"]])) == "atlas")
    #expect(ViewerPresenter.account(ViewerStatus()) == "Helper")
  }

  @Test(
    "Take Control is enabled with a fresh reading of a ready helper, an agent at work included, and waits otherwise"
  )
  func takeControlWaitsForEverything() throws {
    let ready = watching(try status(["ready": true, "overview": ["mode": "observing"]]))
    #expect(ViewerPresenter.state(ready).top.actions.control.isEnabled)
    let agent = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    #expect(ViewerPresenter.state(agent).top.actions.control.isEnabled, "the person may take over from an agent")
    var notReady = ready
    notReady.reading.ready = false
    #expect(!ViewerPresenter.state(notReady).top.actions.control.isEnabled)
    var noFrame = ready
    noFrame.reading.frame = nil
    #expect(!ViewerPresenter.state(noFrame).top.actions.control.isEnabled)
    let elsewhere = watching(try status(["overview": ["mode": "controlling", "state": "control"]]))
    #expect(!ViewerPresenter.state(elsewhere).top.actions.control.isEnabled)
    var paused = ready
    paused.session.paused = true
    #expect(!ViewerPresenter.state(paused).top.actions.control.isEnabled)
    var acquiring = ready
    acquiring.session.acquiring = true
    #expect(!ViewerPresenter.state(acquiring).top.actions.control.isEnabled)
    var controlBusy = ready
    controlBusy.controlBusy = true
    #expect(
      !ViewerPresenter.state(controlBusy).top.actions.control.isEnabled,
      "an acquisition or release is unanswered"
    )
    var commandRunning = ready
    commandRunning.running = [.switchToHelper]
    #expect(!ViewerPresenter.state(commandRunning).top.actions.control.isEnabled, "a helper command runs")
  }

  @Test("the top row offers Stop while an agent acts and Resume once stopped, never while in control")
  func theAgentButtonFollowsTheAgent() throws {
    let agent = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    #expect(ViewerPresenter.state(agent).top.actions.agent == .stop)
    let stopped = watching(try status(["overview": ["mode": "stopped", "state": "stopped"]]))
    #expect(ViewerPresenter.state(stopped).top.actions.agent == .resume(isEnabled: true))
    var recovering = stopped
    recovering.session.recovering = true
    #expect(ViewerPresenter.state(recovering).top.actions.agent == .resume(isEnabled: false))
    let idle = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    #expect(ViewerPresenter.state(idle).top.actions.agent == nil)
    var holding = stopped
    holding.session.lease = "lease"
    #expect(ViewerPresenter.state(holding).top.actions.agent == nil, "no agent acts while this window has control")
  }

  @Test("while the helper needs the person, Recover… takes the white capsule's place and Keys and Stop go")
  func needsYouOffersRecover() throws {
    let snapshot = watching(
      try status([
        "overview": [
          "mode": "recovering", "state": "needsYou", "activity": [["operation": "run", "ok": true, "ageSeconds": 1]],
        ]
      ])
    )
    let actions = ViewerPresenter.state(snapshot).top.actions
    #expect(actions.control.event == .askRecover)
    #expect(actions.keys == .hidden)
    #expect(actions.agent == nil)
  }

  @Test("the control button reads Give Back in control and Connecting… while asked, and Keys is pressed in control")
  func theControlButtonFollowsTheLease() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing"]]))
    #expect(ViewerPresenter.state(snapshot).top.actions.control.title == "Take Control")
    #expect(ViewerPresenter.state(snapshot).top.actions.keys == .disabled)
    snapshot.session.acquiring = true
    #expect(ViewerPresenter.state(snapshot).top.actions.control.title == "Connecting…")
    snapshot.session.acquiring = false
    snapshot.session.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).top.actions.control.event == .giveBack)
    #expect(ViewerPresenter.state(snapshot).top.actions.control.isEnabled)
    #expect(ViewerPresenter.state(snapshot).top.actions.keys == .pressed)
  }

  @Test("captions step aside for control and pause")
  func captionsStepAsideForControlAndPause() throws {
    let base = watching(
      try status(["overview": ["mode": "observing", "activity": [["operation": "run", "ok": true, "ageSeconds": 1]]]])
    )
    #expect(ViewerPresenter.state(base).screen.captions.map(\.line) == ["Ran a command"])
    var control = base
    control.session.lease = "lease"
    #expect(ViewerPresenter.state(control).screen.captions.isEmpty)
    var paused = base
    paused.session.paused = true
    #expect(ViewerPresenter.state(paused).screen.captions.isEmpty)
  }

  @Test(
    "the screen explains a missing picture: logged out, paused, connecting or not ready, with the button that helps"
  )
  func theScreenExplainsAMissingPicture() throws {
    var snapshot = ViewerSnapshot()
    #expect(ViewerPresenter.state(snapshot).screen.empty?.message.headline == ViewerWords.connectingHeadline)
    snapshot.session.paused = true
    #expect(ViewerPresenter.state(snapshot).screen.empty?.message.headline == ViewerWords.pausedHeadline)
    snapshot.session.paused = false
    snapshot.reading.unavailable = "Gone"
    snapshot.reading.status = try status(["overview": ["mode": "unavailable", "state": "notReady"]])
    #expect(ViewerPresenter.state(snapshot).screen.empty?.event == .askRecover)
    snapshot.reading.status = try status(["overview": ["mode": "loggedOut"]])
    #expect(ViewerPresenter.state(snapshot).screen.empty?.event == .askSwitch)
  }

  @Test("typing shows before the control chip, and the paused chip only over a picture")
  func theChipShowsTypingFirst() throws {
    var snapshot = watching(try status(["overview": ["mode": "controlling"]]))
    snapshot.session.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).screen.chip == .inControl)
    snapshot.session.composing = "ni"
    #expect(ViewerPresenter.state(snapshot).screen.chip == .typing("ni"))
    var paused = ViewerSnapshot()
    paused.session.paused = true
    #expect(ViewerPresenter.state(paused).screen.chip == nil, "no picture, no paused chip")
  }

  @Test(
    "the screen in the corner of Activity and Apps reads paused while the picture is, and Screenshots has none",
    arguments: [ViewerTab.activity, .apps, .screenshots]
  )
  func theMiniScreenFollowsThePause(tab: ViewerTab) throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    snapshot.reading.picture = SampleDesktop.picture
    snapshot.session.tab = tab
    snapshot.session.paused = true
    let mini = ViewerPresenter.state(snapshot).mini
    #expect(mini?.isPaused == (tab == .screenshots ? nil : true))
    snapshot.session.paused = false
    #expect(ViewerPresenter.state(snapshot).mini?.isPaused == (tab == .screenshots ? nil : false))
  }

  @Test("while a Recover runs from another window, Recover… waits and the recovery alert shows it running")
  func recoverWaitsForAnotherWindow() throws {
    var snapshot = watching(try status(["overview": ["mode": "recovering", "state": "needsYou"]]))
    snapshot.running = [.recovery]
    let state = ViewerPresenter.state(snapshot)
    #expect(state.top.actions.control.event == .askRecover)
    #expect(!state.top.actions.control.isEnabled)
    #expect(state.alert?.isBusy == true)
  }

  @Test("Ready to act reads as one row while every check passes, else only those that do not and how many others pass")
  func readyRowsSummarise() {
    let ready = ReadinessItem(name: "Helper", text: "Ready", state: .ready)
    let keys = ReadinessItem(name: "Mouse and keyboard", text: "Accessibility is off", state: .notReady)
    let dialogs = ReadinessItem(name: "Save and Open dialogs", text: "Not checked yet", state: .unknown)
    let all = ViewerPresenter.readyRows([ready, ready])
    #expect(all.map(\.name) == ["Ready to act"])
    #expect(all.first?.value == "All checks pass")
    let some = ViewerPresenter.readyRows([ready, keys, dialogs])
    #expect(some.map(\.name) == ["Mouse and keyboard", "Save and Open dialogs", "1 other passes"])
    #expect(some.map(\.mark) == [.notReady, .unknown, nil])
    #expect(some.map(\.note) == ["Accessibility is off", "Not checked yet", nil])
    #expect(ViewerPresenter.readyRows([keys]).map(\.name) == ["Mouse and keyboard"], "no count when none pass")
    #expect(ViewerPresenter.readyRows([]).isEmpty, "nothing before any check is known")
  }
}
