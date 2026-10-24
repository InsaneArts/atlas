import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter bar")
struct ViewerPresenterBarTests {
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
    snapshot.reading.picture = SampleDesktop.picture
    return snapshot
  }

  @Test("the pause leaves the bar while this window holds control, and the View menu's Pause Picture waits with it")
  func thePauseLeavesWhileInControl() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    #expect(ViewerPresenter.state(snapshot).top.actions.pause?.isOn == false)
    snapshot.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).top.actions.pause == nil, "pausing would leave the person working blind")
  }

  @Test("⋯ shows only while there is a picture for its commands to act on")
  func moreShowsOnlyWithAPicture() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    #expect(ViewerPresenter.state(snapshot).top.actions.more?.help == "More")
    snapshot.reading.picture = nil
    #expect(ViewerPresenter.state(snapshot).top.actions.more == nil)
    #expect(ViewerPresenter.state(ViewerSnapshot()).top.actions.more == nil, "connecting")
  }

  @Test("every button's tooltip is its menu item's title and shortcut, and VoiceOver reads the item's title")
  func tooltipsNameTheirShortcuts() throws {
    var snapshot = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    let top = ViewerPresenter.state(snapshot).top
    #expect(top.tabs.tabs.map(\.help) == ["Screen (⌘1)", "Activity (⌘2)", "Apps (⌘3)", "Screenshots (⌘4)"])
    #expect(top.tabs.label == "View")
    #expect(top.actions.agentHelp == "Stop Agent (⌘.)")
    #expect(top.actions.pause?.help == "Pause Picture (⇧⌘P)")
    #expect(top.actions.pause?.label == "Pause Picture")
    #expect(top.actions.details.help == "Show Details (⌥⌘I)")
    #expect(top.actions.details.label == "Show Details")
    #expect(top.actions.control?.help == "Take Control (⌃⇧⎋)")
    snapshot.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).top.actions.control?.help == "Give Back Control (⌃⇧⎋)")
  }

  @Test("the white capsule keeps the width of the widest title it can show, whichever it shows")
  func theCapsuleReservesItsWidestTitle() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    let titles = ["Take Control", "Give Back", "Connecting…", "Recover…"]
    #expect(ViewerPresenter.state(snapshot).top.actions.control?.titles == titles)
    snapshot.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).top.actions.control?.titles == titles)
  }

  @Test("while an agent works the status line goes quiet on the Screen, whose caption says the same, and not elsewhere")
  func theWorkingLineGoesQuietOverTheCaptions() throws {
    var snapshot = watching(
      try status([
        "overview": [
          "mode": "agent", "state": "working",
          "activity": [["operation": "input", "says": "clicking Ring in Opie", "ok": true, "ageSeconds": 1]],
        ]
      ])
    )
    let screen = ViewerPresenter.state(snapshot)
    #expect(screen.top.status.label == "Working")
    #expect(screen.top.status.line.isEmpty, "the caption on the picture says it")
    #expect(!screen.screen.captions.isEmpty)
    snapshot.session.tab = .activity
    #expect(ViewerPresenter.state(snapshot).top.status.line == "Clicking Ring in Opie")
    snapshot.session.tab = .screen
    snapshot.session.paused = true
    #expect(ViewerPresenter.state(snapshot).top.status.line == "Clicking Ring in Opie", "no captions while paused")
  }

  @Test("connecting says the helper is starting the first time and Reconnecting… once a picture was lost")
  func connectingSaysWhetherItIsTheFirstTime() throws {
    var snapshot = ViewerSnapshot()
    #expect(ViewerPresenter.state(snapshot).top.status.line == "Starting the helper…")
    snapshot.reading.lastPicture = SampleDesktop.picture
    let state = ViewerPresenter.state(snapshot)
    #expect(state.top.status.line == "Reconnecting…")
    #expect(state.screen.empty?.message.headline == "Reconnecting…", "the stage says the same word")
  }

  @Test("the Restart alert asks as a destructive one: Cancel is the default and Restart red")
  func restartIsDestructive() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    snapshot.session.alert = .restart
    let alert = try #require(ViewerPresenter.state(snapshot).alert)
    #expect(alert.buttons.map(\.role) == [.safeCancel, .destructive])
    #expect(alert.buttons.last?.title == "Restart")
  }

  @Test("Recover asked by the person offers Cancel, while automatic recovery offers Not Now")
  func recoveryCancellationFollowsWhoAsked() throws {
    var snapshot = watching(try status(["overview": ["mode": "recovering", "state": "needsYou"]]))
    let automatic = ViewerPresenter.alert(.recover, snapshot: snapshot)
    #expect(automatic.buttons.first { $0.id == .cancel }?.title == "Not Now")
    snapshot.session.alert = .recover
    let asked = try #require(ViewerPresenter.alert(snapshot))
    #expect(asked.buttons.first { $0.id == .cancel }?.title == "Cancel")
  }

  @Test("the ⋯ menu ticks the rate the picture resumes at, and acts on a screenshot on the Screenshots tab")
  func theMoreMenuActsOnWhatShows() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    snapshot.rate = .everyFiveSeconds
    snapshot.session.paused = true
    let menu = ViewerPresenter.moreMenu(snapshot)
    #expect(menu.rate == .everyFiveSeconds, "paused, it still ticks the rate it resumes at")
    #expect(!menu.actsOnShot)
    snapshot.session.tab = .screenshots
    snapshot.panes.shots.shots = [Screenshot(name: "a.png", takenAt: nil, width: nil, height: nil)]
    #expect(ViewerPresenter.moreMenu(snapshot).actsOnShot, "the first one is selected")
    snapshot.panes.shots.selectionCleared = true
    #expect(!ViewerPresenter.moreMenu(snapshot).actsOnShot, "Esc cleared the selection")
  }
}
