import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter header and footer")
struct ViewerPresenterFooterTests {
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
    snapshot.reading.picture = SampleDesktop.picture
    return snapshot
  }

  /// An agent's click a second ago.
  static let click: [String: Any] = [
    "operation": "input", "says": "clicking Ring in Opie", "ok": true, "ageSeconds": 1,
  ]

  @Test("the header says you have control while this window holds it")
  func theHeaderSaysYouHaveControl() throws {
    var snapshot = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    #expect(ViewerPresenter.header(snapshot) == ViewerHeaderState(who: "Working", tone: .active))
    snapshot.lease = "lease"
    #expect(ViewerPresenter.header(snapshot) == ViewerHeaderState(who: "You", what: "have control of the helper"))
    snapshot.lease = nil
    snapshot.reading.status = try status(["overview": ["mode": "stopped", "state": "stopped"]])
    #expect(ViewerPresenter.header(snapshot) == ViewerHeaderState(who: "Stopped", tone: .attention))
  }

  @Test("the footer offers Give Back alone while in control")
  func theFooterOffersGiveBackAlone() throws {
    var snapshot = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    let watching = ViewerPresenter.controls(snapshot)
    #expect(watching.pause?.isOn == false)
    #expect(watching.agent == .stop)
    #expect(watching.control?.title == "Take Control")
    #expect(watching.control?.symbol == .handRaised)
    #expect(watching.control?.look == .primary)
    snapshot.lease = "lease"
    let inControl = ViewerPresenter.controls(snapshot)
    #expect(inControl.pause == nil, "pausing would leave the person working blind")
    #expect(inControl.agent == nil, "no agent acts while the person has control")
    #expect(inControl.control == ViewerPresenter.control(snapshot))
    #expect(inControl.control?.title == "Give Back")
    #expect(inControl.control?.look == .accent)
    #expect(inControl.control?.symbol == nil)
    #expect(ViewerPresenter.latest(snapshot).tone == .link, "the footer's line says control with its blue dot")
  }

  @Test("Stop becomes Resume once agents are stopped")
  func stopBecomesResume() throws {
    var snapshot = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    #expect(ViewerPresenter.controls(snapshot).agent == .stop)
    #expect(ViewerPresenter.controls(snapshot).agentHelp == "Stop Agent (⌘.)")
    snapshot.reading.status = try status(["overview": ["mode": "stopped", "state": "stopped"]])
    #expect(ViewerPresenter.controls(snapshot).agent == .resume(isEnabled: true))
    #expect(ViewerPresenter.controls(snapshot).agentHelp == "Resume Agent (⌘.)")
    snapshot.session.recovering = true
    #expect(ViewerPresenter.controls(snapshot).agent == .resume(isEnabled: false), "Resume waits while recovery runs")
  }

  @Test("every button's tooltip is its menu item's title and shortcut, and VoiceOver reads the item's title")
  func tooltipsNameTheirShortcuts() throws {
    var snapshot = watching(try status(["overview": ["mode": "agent", "state": "working"]]))
    let state = ViewerPresenter.state(snapshot)
    #expect(state.sidebar.library.map(\.help) == ["Activity (⌘2)", "Screenshots (⌘3)", "Apps (⌘4)"])
    #expect(state.sidebar.library.map(\.place) == [.activity, .screenshots, .apps])
    #expect(state.footer.controls.pause?.help == "Pause Picture (⇧⌘P)")
    #expect(state.footer.controls.pause?.label == "Pause Picture")
    #expect(state.footer.controls.control?.help == "Take Control (⌃⇧⎋)")
    snapshot.lease = "lease"
    #expect(ViewerPresenter.state(snapshot).footer.controls.control?.help == "Give Back Control (⌃⇧⎋)")
  }

  @Test("the footer says the newest action wherever the window is, paused or not, with its age")
  func theFooterSaysTheNewestAction() throws {
    var snapshot = watching(
      try status(["overview": ["mode": "agent", "state": "working", "activity": [Self.click]]])
    )
    let line = ViewerPresenter.latest(snapshot)
    #expect(line.words == "Clicking Ring in Opie")
    #expect(line.age == "just now")
    #expect(line.icon == nil, "no session is listed, so none is told apart")
    snapshot.session.place = .activity
    #expect(ViewerPresenter.latest(snapshot) == line)
    snapshot.session.paused = true
    #expect(ViewerPresenter.latest(snapshot) == line, "work in the helper goes on while the picture is paused")
    snapshot.reading.status = try status(["overview": ["mode": "observing", "state": "idle"]])
    #expect(ViewerPresenter.latest(snapshot).words == ViewerPresenter.status(snapshot).line, "else the helper's line")
  }

  @Test("a refused Take Control's reason takes the footer's line over the newest action, in orange, and is said aloud")
  func aRefusalTakesTheFootersLine() throws {
    var snapshot = watching(
      try status(["overview": ["mode": "agent", "state": "working", "activity": [Self.click]]])
    )
    snapshot.session.refusals = 1
    snapshot.session.toast = ViewerWords.agentMidAction
    let line = ViewerPresenter.latest(snapshot)
    #expect(line.words == ViewerWords.agentMidAction)
    #expect(line.tone == .attention)
    #expect(line.announces)
    snapshot.session.refusals = 2
    #expect(ViewerPresenter.latest(snapshot).id != line.id, "each refusal is a line of its own")
  }

  @Test("a notice holds the footer's line until an action newer than it is done")
  func aNoticeHoldsUntilANewerAction() throws {
    let earlier: [String: Any] = ["id": "a1", "operation": "input", "says": "scrolled", "ok": true, "ageSeconds": 2]
    var snapshot = watching(
      try status(["overview": ["mode": "agent", "state": "working", "activity": [earlier]]])
    )
    snapshot.session.notice = ViewerWords.agentResumed
    snapshot.session.noticeAction = snapshot.reading.statusReading.activity(limit: 1).first?.id
    #expect(ViewerPresenter.latest(snapshot).words == ViewerWords.agentResumed)
    #expect(ViewerPresenter.latest(snapshot).announces)
    let newer: [String: Any] = ["id": "a2", "operation": "input", "says": "clicked Ring", "ok": true, "ageSeconds": 0]
    snapshot.reading.status = try status([
      "overview": ["mode": "agent", "state": "working", "activity": [newer, earlier]]
    ])
    #expect(ViewerPresenter.latest(snapshot).words == "Clicked Ring", "the newer action takes the line back")
  }

  @Test("connecting says the helper is starting the first time and Reconnecting… once a picture was lost")
  func connectingSaysWhetherItIsTheFirstTime() throws {
    var snapshot = ViewerSnapshot()
    #expect(ViewerPresenter.state(snapshot).footer.latest.words == "Starting the helper…")
    #expect(ViewerPresenter.state(snapshot).header.who == "Connecting")
    snapshot.reading.lastPicture = SampleDesktop.picture
    let state = ViewerPresenter.state(snapshot)
    #expect(state.footer.latest.words == "Reconnecting…")
    #expect(state.screen.empty?.message.headline == "Reconnecting…", "the stage says the same word")
  }

  @Test("the stage's menu offers Copy, Save, Actual Size, Pause and the update rate only while not in control")
  func theStagesMenuOffersThePicturesCommands() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    snapshot.rate = .everyFiveSeconds
    snapshot.session.actualSize = true
    let menu = ViewerPresenter.screenMenu(snapshot)
    let titles = menu.map { $0.items.map(\.title) }
    #expect(titles == [["Copy Picture", "Save Picture…"], ["Actual Size", "Pause Picture", "Picture Updates"]])
    let commands = menu.flatMap(\.items)
      .compactMap(\.command)
    #expect(commands == [.copyPicture, .savePicture, .toggleActualSize, .togglePause])
    let rates = try #require(menu.last?.items.last?.children)
    #expect(rates.map(\.title) == ["Live", "Every 5 Seconds"])
    #expect(rates.map(\.command) == [.chooseRate(index: 0), .chooseRate(index: 1)])
    #expect(rates.map(\.isOn) == [false, true])
    #expect(menu.flatMap(\.items).filter(\.isOn).map(\.title) == ["Actual Size"])
    snapshot.session.paused = true
    #expect(ViewerPresenter.screenMenu(snapshot).last?.items[1].title == "Resume Picture")
    snapshot.session.paused = false
    snapshot.lease = "lease"
    #expect(ViewerPresenter.screenMenu(snapshot).isEmpty, "in control a right click is the helper's")
    snapshot.lease = nil
    snapshot.reading.picture = nil
    #expect(ViewerPresenter.screenMenu(snapshot).isEmpty, "nothing to act on")
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

  @Test("the picture's commands tick the rate the picture resumes at, and act on a screenshot in the Screenshots list")
  func thePicturesCommandsActOnWhatShows() throws {
    var snapshot = watching(try status(["overview": ["mode": "observing", "state": "idle"]]))
    snapshot.rate = .everyFiveSeconds
    snapshot.session.paused = true
    let menu = ViewerPresenter.pictureMenu(snapshot)
    #expect(menu.rate == .everyFiveSeconds, "paused, it still ticks the rate it resumes at")
    #expect(!menu.actsOnShot)
    snapshot.session.place = .screenshots
    snapshot.panes.shots.shots = [Screenshot(name: "a.png", takenAt: nil, width: nil, height: nil)]
    #expect(ViewerPresenter.pictureMenu(snapshot).actsOnShot, "the first one is selected")
    snapshot.panes.shots.selectionCleared = true
    #expect(!ViewerPresenter.pictureMenu(snapshot).actsOnShot, "Esc cleared the selection")
  }
}
