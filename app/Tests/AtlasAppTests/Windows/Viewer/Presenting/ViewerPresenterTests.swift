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
    snapshot.reading.frame = (id: "f", revision: "r", display: ViewerStatus.Size(width: 4, height: 3))
    snapshot.reading.frameAt = 0
    return snapshot
  }

  @Test(
    "the status reads Connecting before the first reading and while the picture is lost, unless the helper's own state says more"
  )
  func theStatusReadsConnectingFirst() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    #expect(ViewerPresenter.status(snapshot).label == "Idle")
    snapshot.session.paused = true
    #expect(
      ViewerPresenter.status(snapshot).line == Words.readyLine,
      "a paused picture keeps the agent's own line; the header says it is paused"
    )
    snapshot.reading.unavailable = "Gone"
    #expect(ViewerPresenter.status(snapshot).label == "Connecting")
    snapshot.reading.status = try status(["overview": ["mode": "stopped", "state": "stopped"]])
    #expect(ViewerPresenter.status(snapshot).label == "Stopped", "a stop the window knows of stays")
    #expect(ViewerPresenter.status(ViewerSnapshot()).label == "Connecting")
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
    "Take Control is enabled with a fresh reading of a ready helper, an agent at work or a paused picture included, and waits otherwise"
  )
  func takeControlWaitsForEverything() throws {
    let ready = watching(try status(["ready": true, "overview": ["mode": "observing"]]))
    #expect(ViewerPresenter.state(ready).footer.controls.control?.isEnabled == true)
    let agent = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    #expect(
      ViewerPresenter.state(agent).footer.controls.control?.isEnabled == true,
      "the person may take over from an agent"
    )
    var notReady = ready
    notReady.reading.ready = false
    #expect(ViewerPresenter.state(notReady).footer.controls.control?.isEnabled != true)
    var noFrame = ready
    noFrame.reading.frame = nil
    #expect(ViewerPresenter.state(noFrame).footer.controls.control?.isEnabled != true)
    let elsewhere = watching(try status(["overview": ["mode": "controlling", "state": "control"]]))
    #expect(ViewerPresenter.state(elsewhere).footer.controls.control?.isEnabled != true)
    var paused = ready
    paused.session.paused = true
    #expect(
      ViewerPresenter.state(paused).footer.controls.control?.isEnabled == true,
      "pressed while paused, it resumes the picture first"
    )
    var acquiring = ready
    acquiring.acquiring = true
    #expect(ViewerPresenter.state(acquiring).footer.controls.control?.isEnabled != true)
    var controlBusy = ready
    controlBusy.controlBusy = true
    #expect(
      ViewerPresenter.state(controlBusy).footer.controls.control?.isEnabled != true,
      "an acquisition or release is unanswered"
    )
    var commandRunning = ready
    commandRunning.running = [.switchToHelper]
    #expect(ViewerPresenter.state(commandRunning).footer.controls.control?.isEnabled != true, "a helper command runs")
  }

  @Test("the footer offers Stop while an agent acts and Resume once stopped, never while in control")
  func theAgentButtonFollowsTheAgent() throws {
    let agent = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    #expect(ViewerPresenter.state(agent).footer.controls.agent == .stop)
    let stopped = watching(try status(["overview": ["mode": "stopped", "state": "stopped"]]))
    #expect(ViewerPresenter.state(stopped).footer.controls.agent == .resume(isEnabled: true))
    var recovering = stopped
    recovering.session.recovering = true
    #expect(ViewerPresenter.state(recovering).footer.controls.agent == .resume(isEnabled: false))
    let idle = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    #expect(ViewerPresenter.state(idle).footer.controls.agent == nil)
    var holding = stopped
    holding.lease = "lease"
    #expect(ViewerPresenter.state(holding).footer.controls.agent == nil, "no agent acts while this window has control")
  }

  @Test("while the helper needs the person, Recover… takes the main capsule's place and Stop goes")
  func needsYouOffersRecover() throws {
    let snapshot = watching(
      try status([
        "overview": [
          "mode": "recovering", "state": "needsYou", "activity": [["operation": "run", "ok": true, "ageSeconds": 1]],
        ]
      ])
    )
    let controls = ViewerPresenter.state(snapshot).footer.controls
    #expect(controls.control?.event == .askRecover)
    #expect(controls.control?.symbol == nil)
    #expect(controls.agent == nil)
  }

  @Test("the control button reads Give Back in control and Connecting… while asked")
  func theControlButtonFollowsTheLease() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing"]]))
    #expect(ViewerPresenter.state(snapshot).footer.controls.control?.title == "Take Control")
    snapshot.acquiring = true
    #expect(ViewerPresenter.state(snapshot).footer.controls.control?.title == "Connecting…")
    snapshot.acquiring = false
    snapshot.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).footer.controls.control?.event == .giveBack)
    #expect(ViewerPresenter.state(snapshot).footer.controls.control?.isEnabled == true)
  }

  @Test("a logged-out helper leaves Take Control off the footer")
  func aLoggedOutHelperLeavesTakeControlOff() throws {
    var loggedOut = watching(try status(["overview": ["mode": "loggedOut", "state": "notReady"]]))
    loggedOut.reading.picture = nil
    loggedOut.reading.unavailable = "Gone"
    let state = ViewerPresenter.state(loggedOut)
    #expect(state.header.who == "Logged out")
    #expect(state.footer.controls.control == nil, "nothing to take, so nothing drawn")
    #expect(state.footer.controls.pause == nil, "no picture to pause")
    #expect(state.screen.empty?.event == .askSwitch, "the stage offers the way back")
    var notReady = loggedOut
    notReady.reading.status = try status(["overview": ["mode": "unavailable", "state": "notReady"]])
    notReady.reading.ready = false
    #expect(ViewerPresenter.state(notReady).footer.controls.control == nil)
    #expect(ViewerPresenter.state(ViewerSnapshot()).footer.controls.control == nil, "connecting")
    let idle = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    #expect(ViewerPresenter.state(idle).footer.controls.control?.event == .takeControl, "a ready helper offers it")
  }

  @Test("control's line steps in for the latest action while this window holds control")
  func controlsLineStepsInForTheLatestAction() throws {
    let base = watching(
      try status(["overview": ["mode": "observing", "activity": [["operation": "run", "ok": true, "ageSeconds": 1]]]])
    )
    #expect(ViewerPresenter.state(base).footer.latest.words == "Ran a command")
    var control = base
    control.lease = "lease"
    #expect(ViewerPresenter.state(control).footer.latest.tone == .link)
    #expect(ViewerPresenter.state(control).footer.latest.words == control.reading.statusReading.controlLine)
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
    #expect(ViewerPresenter.state(snapshot).screen.empty?.event == .openSettings)
    snapshot.reading.status = try status(["overview": ["mode": "loggedOut"]])
    #expect(ViewerPresenter.state(snapshot).screen.empty?.event == .askSwitch)
  }

  @Test("a permission failure offers Settings and its guidance instead of recovery")
  func aPermissionFailureOffersSettings() throws {
    var snapshot = ViewerSnapshot()
    snapshot.reading.status = try status([
      "ready": false, "canObserve": false, "nextSteps": ["Allow Screen Recording."],
      "overview": [
        "mode": "blocked", "state": "notReady", "recoverable": false,
        "reason": "Screen Recording is off for Atlas in the helper.",
      ],
    ])
    snapshot.reading.hasStatus = true
    let empty = try #require(ViewerPresenter.state(snapshot).screen.empty)
    #expect(empty.message.headline == ViewerWords.notReadyLine(snapshot.reading.status.overview.reason))
    #expect(empty.message.message == ViewerWords.notReadyWaitMessage)
    #expect(empty.message.action?.title == ViewerWords.openSettings)
    #expect(empty.event == .openSettings)
    #expect(!snapshot.helperMenu.canRecover)
  }

  @Test("an input failure that recovery can fix offers Recover")
  func aRecoverableInputFailureOffersRecover() throws {
    var snapshot = ViewerSnapshot()
    snapshot.reading.status = try status([
      "ready": false, "canObserve": true,
      "overview": [
        "mode": "blocked", "state": "notReady", "recoverable": true,
        "reason": "Mouse and keyboard aren't ready.",
      ],
    ])
    snapshot.reading.hasStatus = true
    let empty = try #require(ViewerPresenter.state(snapshot).screen.empty)
    #expect(empty.message.headline == ViewerWords.notReadyHeadline)
    #expect(empty.message.message == ViewerWords.notReadyMessage)
    #expect(empty.message.action?.title == ViewerWords.recoverButton)
    #expect(empty.event == .askRecover)
  }

  @Test("typing shows before control's line, and in place as it is typed")
  func typingShowsBeforeControl() throws {
    var snapshot = watching(try status(["overview": ["mode": "controlling"]]))
    snapshot.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).footer.latest.tone == .link)
    snapshot.composing = "ni"
    let typing = ViewerPresenter.state(snapshot).footer.latest
    #expect(typing.words == "Typing ni")
    #expect(typing.caret)
    snapshot.composing = "nih"
    #expect(ViewerPresenter.state(snapshot).footer.latest.id == typing.id, "a longer composition updates in place")
  }

  @Test(
    "the screen in the corner of every list reads paused while the picture is",
    arguments: [ViewerPlace.activity, .screenshots, .apps]
  )
  func theMiniScreenFollowsThePause(place: ViewerPlace) throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    snapshot.reading.picture = SampleDesktop.picture
    snapshot.session.place = place
    snapshot.session.paused = true
    #expect(ViewerPresenter.state(snapshot).mini?.isPaused == true)
    snapshot.session.paused = false
    #expect(ViewerPresenter.state(snapshot).mini?.isPaused == false)
    snapshot.session.place = .screen
    #expect(ViewerPresenter.state(snapshot).mini == nil, "the Screen is the screen")
  }

  @Test("while a Recover runs from another window, Recover… waits and the recovery alert shows it running")
  func recoverWaitsForAnotherWindow() throws {
    var snapshot = watching(try status(["overview": ["mode": "recovering", "state": "needsYou"]]))
    snapshot.running = [.recovery]
    let state = ViewerPresenter.state(snapshot)
    #expect(state.footer.controls.control?.event == .askRecover)
    #expect(state.footer.controls.control?.isEnabled != true)
    #expect(state.alert?.isBusy == true)
  }

  @Test("Ready to act reads as one fact while every check passes, else only those that do not and how many others pass")
  func readyFactsSummarise() {
    let ready = ReadinessItem(name: "Helper", text: "Ready", state: .ready)
    let keys = ReadinessItem(name: "Mouse and keyboard", text: "Accessibility is off", state: .notReady)
    let dialogs = ReadinessItem(name: "Save and Open dialogs", text: "Not checked yet", state: .unknown)
    #expect(ViewerPresenter.readyFacts([ready, ready]) == [Fact(name: "Ready to act", value: "All checks pass")])
    #expect(
      ViewerPresenter.readyFacts([ready, keys, dialogs]) == [
        Fact(name: "Mouse and keyboard", value: "Accessibility is off"),
        Fact(name: "Save and Open dialogs", value: "Not checked yet"),
        Fact(name: "1 more check passes", value: ""),
      ]
    )
    #expect(ViewerPresenter.readyFacts([keys]).map(\.name) == ["Mouse and keyboard"], "no count when none pass")
    #expect(ViewerPresenter.readyFacts([]).isEmpty, "nothing before any check is known")
  }
}
