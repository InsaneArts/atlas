import AtlasKit
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@MainActor
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

  @Test("the header reads Paused, then Reconnecting, then Connecting before the helper's own mode")
  func theHeaderReadsTheWindowsOwnStateFirst() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing"]]))
    #expect(ViewerPresenter.state(snapshot).header.status.label == "Watching")
    snapshot.reading.unavailable = "Gone"
    #expect(ViewerPresenter.state(snapshot).header.status.label == "Reconnecting")
    snapshot.session.paused = true
    #expect(ViewerPresenter.state(snapshot).header.status.label == "Paused")
    #expect(ViewerPresenter.state(ViewerSnapshot()).header.status.label == "Connecting")
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

  @Test("the freshness reads Picture paused or No picture before the picture's age")
  func theFreshnessExplainsAMissingPictureFirst() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing"]]))
    #expect(
      ViewerPresenter.freshness(reading: snapshot.reading, session: snapshot.session, now: snapshot.now) == "Live"
    )
    snapshot.session.paused = true
    #expect(
      ViewerPresenter.freshness(reading: snapshot.reading, session: snapshot.session, now: snapshot.now)
        == "Picture paused"
    )
    snapshot.session.paused = false
    snapshot.reading.frame = nil
    snapshot.reading.unavailable = "Gone"
    #expect(
      ViewerPresenter.freshness(reading: snapshot.reading, session: snapshot.session, now: snapshot.now) == "No picture"
    )
  }

  @Test("Take Control is enabled only with a fresh reading, a ready helper and nothing blocking")
  func takeControlWaitsForEverything() throws {
    let ready = watching(try status(["ready": true, "overview": ["mode": "observing"]]))
    #expect(ViewerPresenter.state(ready).footer.control.isEnabled)
    var notReady = ready
    notReady.reading.ready = false
    #expect(!ViewerPresenter.state(notReady).footer.control.isEnabled)
    var noFrame = ready
    noFrame.reading.frame = nil
    #expect(!ViewerPresenter.state(noFrame).footer.control.isEnabled)
    let agent = watching(try status(["overview": ["mode": "agent"]]))
    #expect(!ViewerPresenter.state(agent).footer.control.isEnabled)
    var paused = ready
    paused.session.paused = true
    #expect(!ViewerPresenter.state(paused).footer.control.isEnabled)
  }

  @Test("the control button reads Give Back in control and Connecting… while asked")
  func theControlButtonFollowsTheLease() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing"]]))
    #expect(ViewerPresenter.state(snapshot).footer.control.title == "Take Control")
    snapshot.session.acquiring = true
    #expect(ViewerPresenter.state(snapshot).footer.control.title == "Connecting…")
    snapshot.session.acquiring = false
    snapshot.session.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).footer.control.event == .giveBack)
    #expect(ViewerPresenter.state(snapshot).footer.control.isEnabled)
  }

  @Test("captions step aside for control and pause, and stay for a sidebar that is only wanted")
  func captionsStepAsideForControlAndPause() throws {
    let base = watching(
      try status(["overview": ["mode": "observing", "activity": [["operation": "run", "ok": true, "ageSeconds": 1]]]])
    )
    #expect(ViewerPresenter.state(base).stage.captions.map(\.line) == ["Agent ran a command"])
    var sidebar = base
    sidebar.sidebarShown = true
    #expect(
      ViewerPresenter.state(sidebar).stage.captions == ViewerPresenter.state(base).stage.captions,
      "whether the sidebar shows is the window's width, which the view knows"
    )
    var control = base
    control.session.lease = "lease"
    #expect(ViewerPresenter.state(control).stage.captions.isEmpty)
    var paused = base
    paused.session.paused = true
    #expect(ViewerPresenter.state(paused).stage.captions.isEmpty)
  }

  @Test("the stage explains a missing picture: logged out, unavailable, paused or connecting")
  func theStageExplainsAMissingPicture() throws {
    var snapshot = ViewerSnapshot()
    #expect(ViewerPresenter.state(snapshot).stage.placeholder == .connecting)
    snapshot.session.paused = true
    #expect(ViewerPresenter.state(snapshot).stage.placeholder == .paused)
    snapshot.reading.unavailable = "Gone"
    #expect(ViewerPresenter.state(snapshot).stage.placeholder == .unavailable(reason: "Gone"))
    snapshot.reading.status = try status(["overview": ["mode": "loggedOut"]])
    snapshot.canLogIn = true
    #expect(ViewerPresenter.state(snapshot).stage.placeholder == .loggedOut(text: Words.loggedOut, canLogIn: true))
  }

  @Test("typing shows before the control chip, and the paused chip only over a picture")
  func theChipShowsTypingFirst() throws {
    var snapshot = watching(try status(["overview": ["mode": "controlling"]]))
    snapshot.session.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).stage.chip == .inControl)
    snapshot.session.composing = "ni"
    #expect(ViewerPresenter.state(snapshot).stage.chip == .typing("ni"))
    var paused = ViewerSnapshot()
    paused.session.paused = true
    #expect(ViewerPresenter.state(paused).stage.chip == nil, "no picture, no paused chip")
  }

  @Test("the hint says what the person can do now")
  func theHintSaysWhatToDoNow() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing"]]))
    #expect(ViewerPresenter.state(snapshot).footer.hint == ViewerWords.watching)
    snapshot.session.paused = true
    #expect(ViewerPresenter.state(snapshot).footer.hint == ViewerWords.pausedHint)
    snapshot.session.paused = false
    snapshot.session.acquiring = true
    #expect(ViewerPresenter.state(snapshot).footer.hint == ViewerWords.connectingControl)
    snapshot.session.acquiring = false
    snapshot.session.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).footer.hint == ViewerWords.clickToConnect)
    snapshot.session.keyboardConnected = true
    #expect(ViewerPresenter.state(snapshot).footer.hint == ViewerWords.keyboardConnected)
    snapshot.session.lease = nil
    snapshot.reading.ready = false
    #expect(ViewerPresenter.state(snapshot).footer.hint == ViewerWords.inputNotReady)
    snapshot.reading.unavailable = "Gone"
    #expect(ViewerPresenter.state(snapshot).footer.hint == ViewerWords.waitingForPicture)
  }
}
