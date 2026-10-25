import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("ViewerWindow menus", .serialized, AppKitState())
struct ViewerWindowMenuTests {
  /// A hidden native shell: menu validation reaches the real window without presenting a desktop test window.
  func window(_ viewer: ViewerController) -> ViewerWindow {
    _ = NSApplication.shared
    return ViewerWindow(controller: viewer) { _ in }
  }

  /// An item whose title and tick must be set by the window's validation.
  func item(_ action: Selector, tag: Int = 0) -> NSMenuItem {
    let item = NSMenuItem(title: "Before validation", action: action, keyEquivalent: "")
    item.tag = tag
    return item
  }

  @Test("native tab, Details, rate and size items keep their ticks, titles and actions")
  func tabsDetailsRateAndSizeFollowTheWindow() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    for selected in ViewerTab.allCases {
      let choice = item(#selector(ViewerMenuActions.selectTab(_:)), tag: selected.menuTag)
      window.selectTab(choice)
      #expect(viewer.session.tab == selected)
      for tab in ViewerTab.allCases {
        let candidate = item(#selector(ViewerMenuActions.selectTab(_:)), tag: tab.menuTag)
        #expect(window.validateMenuItem(candidate))
        #expect(candidate.state == (tab == selected ? .on : .off))
      }
    }
    #expect(!window.validateMenuItem(item(#selector(ViewerMenuActions.selectTab(_:)), tag: -1)))
    let details = item(#selector(ViewerMenuActions.toggleDetails(_:)))
    #expect(window.validateMenuItem(details))
    #expect(details.title == ViewerWords.showDetailsItem)
    window.toggleDetails(nil)
    #expect(viewer.details.shown)
    #expect(window.validateMenuItem(details))
    #expect(details.title == ViewerWords.hideDetailsItem)
    let rate = item(#selector(ViewerMenuActions.chooseRate(_:)), tag: ViewerPictureRate.everyFiveSeconds.menuTag)
    #expect(window.validateMenuItem(rate))
    #expect(rate.state == .off)
    window.chooseRate(rate)
    #expect(window.validateMenuItem(rate))
    #expect(rate.state == .on)
    let size = item(#selector(ViewerMenuActions.toggleActualSize(_:)))
    #expect(window.validateMenuItem(size))
    #expect(size.state == .off)
    window.toggleActualSize(nil)
    #expect(window.validateMenuItem(size))
    #expect(size.state == .on)
  }

  @Test("native control and pause items preserve paused, alert, controlling and releasing decisions")
  func controlAndPauseFollowTheirLifetimes() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    let window = window(viewer)
    defer {
      service.resume("release")
      window.close()
    }
    let control = item(#selector(ViewerMenuActions.toggleControl(_:)))
    let pause = item(#selector(ViewerMenuActions.togglePicturePause(_:)))
    #expect(window.validateMenuItem(control))
    #expect(control.title == ViewerWords.takeControl)
    #expect(window.validateMenuItem(pause))
    #expect(pause.title == Words.pausePictureItem)
    window.togglePicturePause(nil)
    #expect(viewer.session.paused)
    #expect(window.validateMenuItem(pause))
    #expect(pause.title == Words.resumePictureItem)
    #expect(window.validateMenuItem(control))
    viewer.session.alert = .restart
    #expect(!window.validateMenuItem(control))
    #expect(window.validateMenuItem(pause), "an alert does not change the existing Pause menu policy")
    viewer.session.alert = nil
    window.togglePicturePause(nil)
    try await settle("resuming never supplied a picture") { viewer.reading.frame != nil }
    viewer.control.gainedFocus()
    await viewer.control.takeControl()
    try #require(viewer.holdingControl)
    #expect(window.validateMenuItem(control))
    #expect(control.title == ViewerWords.giveBackItem)
    #expect(!window.validateMenuItem(pause))
    window.togglePicturePause(nil)
    #expect(!viewer.session.paused, "the action guard also prevents pausing in control")
    service.hold.insert("release")
    window.toggleControl(nil)
    try await settle("Give Back never requested release") { service.held["release"] != nil }
    #expect(!viewer.holdingControl)
    #expect(!window.validateMenuItem(control), "another acquisition waits for release")
    #expect(control.title == ViewerWords.takeControl)
    #expect(window.validateMenuItem(pause))
    service.resume("release")
    try await settle("release never settled") { !viewer.barrier.isBusy }
    #expect(window.validateMenuItem(control))
  }

  @Test("native Stop and Resume items retain busy suppression, alert gating and Needs You suppression")
  func agentItemsKeepTheirSuppression() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    let window = window(viewer)
    defer {
      service.resume("hold")
      window.close()
    }
    let agent = item(#selector(ViewerMenuActions.toggleAgent(_:)))
    viewer.readingLoop.show(try status(["ready": true, "overview": ["mode": "agent", "state": "working"]]))
    #expect(window.validateMenuItem(agent))
    #expect(agent.title == ViewerWords.stopAgentItem)
    viewer.session.alert = .restart
    #expect(!window.validateMenuItem(agent))
    viewer.session.alert = nil
    service.hold.insert("hold")
    window.toggleAgent(nil)
    try await settle("Stop never reached the service") { service.held["hold"] != nil }
    #expect(service.log == ["stop"])
    viewer.readingLoop.show(try status(["ready": true, "overview": ["mode": "stopped", "state": "stopped"]]))
    #expect(!window.validateMenuItem(agent))
    #expect(agent.title == ViewerWords.resumeAgentItem)
    window.toggleAgent(nil)
    #expect(service.log == ["stop"], "a blocked Resume sends nothing")
    service.resume("hold")
    try await settle("Stop never settled") { viewer.actions.agentRequests == 0 }
    #expect(window.validateMenuItem(agent))
    window.toggleAgent(nil)
    try await settle("Resume never reached the service") { service.log.contains("resume") }
    try await settle("Resume never settled") { viewer.actions.agentRequests == 0 }
    viewer.readingLoop.show(
      try status(["ready": true, "overview": ["mode": "recovering", "state": "needsYou", "pending": 1]])
    )
    #expect(!window.validateMenuItem(agent))
    #expect(agent.title == ViewerWords.stopAgentItem)
    window.toggleAgent(nil)
    #expect(service.log == ["stop", "resume"])
  }

  @Test("native helper items retain their account titles and alert gating")
  func helperItemsUseTheAccountAndAlert() throws {
    let viewer = ViewerController.scripted(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    let switching = item(#selector(HelperMenuActions.askSwitch(_:)))
    let logOut = item(#selector(HelperMenuActions.askLogOut(_:)))
    let restart = item(#selector(HelperMenuActions.askRestart(_:)))
    let recover = item(#selector(HelperMenuActions.askRecover(_:)))
    #expect(window.validateMenuItem(switching))
    #expect(switching.title == MenuWords.switchTo(nil))
    #expect(!window.validateMenuItem(restart))
    #expect(!window.validateMenuItem(recover))
    viewer.readingLoop.show(
      try status(["overview": ["mode": "observing", "accountName": "Test Helper", "account": "test-helper"]])
    )
    #expect(window.validateMenuItem(switching))
    #expect(switching.title == MenuWords.switchTo("Test Helper"))
    #expect(window.validateMenuItem(logOut))
    #expect(logOut.title == MenuWords.logOut("Test Helper"))
    #expect(window.validateMenuItem(restart))
    viewer.session.alert = .restart
    for item in [switching, logOut, restart, recover] { #expect(!window.validateMenuItem(item)) }
    viewer.session.alert = nil
    viewer.readingLoop.show(try status(["overview": ["mode": "recovering", "pending": 1]]))
    #expect(!window.validateMenuItem(recover), "automatic recovery is an alert too")
    viewer.cancelAlert()
    #expect(window.validateMenuItem(recover))
    window.askRecover(nil)
    #expect(viewer.session.alert == .recover)
  }

  @Test("native Find follows empty and filtered Activity and Apps while Screen and Screenshots stay unsearchable")
  func findUsesThePaneThatIsShown() async throws {
    let service = ScriptedService()
    let viewer = ViewerController.scripted(service)
    let window = window(viewer)
    defer { window.close() }
    let find = item(#selector(ViewerMenuActions.findInTab(_:)))
    #expect(!window.validateMenuItem(find))
    viewer.session.tab = .activity
    #expect(!window.validateMenuItem(find))
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    #expect(!window.validateMenuItem(find))
    service.activityLog = ActivityLog(entries: [ActivityLog.Entry(id: "one", says: "Opened a window")])
    await viewer.panesModel.readActivity(viewer.panesModel.work.generation)
    viewer.send(.searchActivity("unmatched"))
    viewer.send(.filterActivity(.problems))
    #expect(window.validateMenuItem(find), "filtering every row out keeps the search field")
    window.findInTab(nil)
    #expect(viewer.panes.activity.findRequests == 1)
    viewer.session.tab = .apps
    #expect(!window.validateMenuItem(find))
    await viewer.panesModel.readApps(viewer.panesModel.work.generation)
    #expect(!window.validateMenuItem(find))
    service.openApps = [HelperApp(pid: 42, name: "Test App")]
    await viewer.panesModel.readApps(viewer.panesModel.work.generation)
    viewer.send(.searchApps("unmatched"))
    #expect(window.validateMenuItem(find))
    window.findInTab(nil)
    #expect(viewer.panes.apps.findRequests == 1)
    viewer.session.tab = .screenshots
    #expect(!window.validateMenuItem(find))
  }

  @Test("native Copy and Save follow the selected screenshot and fall back to the live picture after clearing")
  func copyAndSaveFollowTheSelection() async throws {
    let service = ScriptedService()
    let viewer = ViewerController.scripted(service)
    let window = window(viewer)
    defer { window.close() }
    let copy = item(#selector(NSText.copy(_:)))
    let save = item(#selector(ViewerMenuActions.savePicture(_:)))
    #expect(!window.validateMenuItem(copy))
    #expect(!window.validateMenuItem(save))
    #expect(save.title == ViewerWords.savePicture)
    viewer.session.tab = .screenshots
    service.shots = [Screenshot(name: "one.png")]
    await viewer.panesModel.readShots(viewer.panesModel.work.generation)
    #expect(window.validateMenuItem(copy))
    #expect(window.validateMenuItem(save))
    #expect(save.title == ViewerWords.saveScreenshot)
    viewer.send(.clearShotSelection)
    #expect(!window.validateMenuItem(copy))
    #expect(!window.validateMenuItem(save))
    #expect(save.title == ViewerWords.savePicture)
    viewer.readingLoop.begin(with: nil, picture: SampleDesktop.picture)
    #expect(window.validateMenuItem(copy))
    #expect(window.validateMenuItem(save))
    #expect(save.title == ViewerWords.savePicture)
    viewer.send(.selectShot("one.png"))
    #expect(window.validateMenuItem(save))
    #expect(save.title == ViewerWords.saveScreenshot)
  }
}
