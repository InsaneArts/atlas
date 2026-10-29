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

  @Test("native tab, rate and size items keep their ticks, titles and actions")
  func tabsRateAndSizeFollowTheWindow() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    for selected in ViewerPlace.allCases {
      let choice = item(#selector(ViewerMenuActions.showPlace(_:)), tag: selected.menuTag)
      window.showPlace(choice)
      #expect(viewer.session.place == selected)
      for place in ViewerPlace.allCases {
        let candidate = item(#selector(ViewerMenuActions.showPlace(_:)), tag: place.menuTag)
        #expect(window.validateMenuItem(candidate))
        #expect(candidate.state == (place == selected ? .on : .off))
      }
    }
    #expect(!window.validateMenuItem(item(#selector(ViewerMenuActions.showPlace(_:)), tag: -1)))
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

  @Test("Screen, Activity, Screenshots and Apps take ⌘1 to ⌘4 in the sidebar's order")
  func placesTakeTheirNumbersInTheSidebarsOrder() throws {
    let places = MainMenu.view().items
      .filter { $0.action == #selector(ViewerMenuActions.showPlace(_:)) }
    #expect(places.map(\.title) == ["Screen", "Activity", "Screenshots", "Apps"])
    #expect(places.map(\.keyEquivalent) == ["1", "2", "3", "4"])
    #expect(places.compactMap { ViewerPlace(menuTag: $0.tag) } == [.screen, .activity, .screenshots, .apps])
    #expect(places.allSatisfy { $0.keyEquivalentModifierMask == .command })
  }

  @Test("Send Key is enabled only while in control")
  func sendKeyIsEnabledOnlyInControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    let window = window(viewer)
    defer { window.close() }
    let keys = HelperKey.all.indices
      .map { item(#selector(ViewerMenuActions.sendKey(_:)), tag: $0) }
    #expect(keys.allSatisfy { !window.validateMenuItem($0) })
    viewer.control.gainedFocus()
    await viewer.control.takeControl()
    try #require(viewer.holdingControl)
    #expect(keys.allSatisfy { window.validateMenuItem($0) })
    #expect(!window.validateMenuItem(item(#selector(ViewerMenuActions.sendKey(_:)), tag: HelperKey.all.count)))
    window.sendKey(keys[0])
    try await settle("the key never reached the helper") { service.actions == [[.key("cmd+tab")]] }
    viewer.control.releaseControl()
    try await settle("control never went back") { !viewer.holdingControl }
    #expect(keys.allSatisfy { !window.validateMenuItem($0) })
  }

  @Test("Picture Updates ticks the rate the picture resumes at")
  func pictureUpdatesTicksTheRateItResumesAt() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    let updates = try #require(
      MainMenu.view().items
        .first { $0.title == ViewerWords.updatesItem }?
        .submenu
    )
    #expect(updates.items.map(\.title) == ["Live", "Every 5 Seconds"])
    #expect(updates.items.allSatisfy { $0.action == #selector(ViewerMenuActions.chooseRate(_:)) })
    let every = try #require(updates.items.last)
    window.chooseRate(every)
    viewer.togglePause()
    #expect(viewer.session.paused)
    for choice in updates.items {
      #expect(window.validateMenuItem(choice))
      #expect(choice.state == (choice === every ? .on : .off), "\(choice.title) while paused")
    }
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

  @Test("native Find opens the sidebar's search wherever the window is, showing a hidden sidebar first")
  func findOpensTheSidebarSearch() async throws {
    let service = ScriptedService()
    let viewer = ViewerController.scripted(service)
    let window = window(viewer)
    defer { window.close() }
    let find = item(#selector(ViewerMenuActions.findInTab(_:)))
    viewer.send(.toggleSidebar)
    #expect(window.validateMenuItem(find), "the Screen searches the sidebar")
    window.findInTab(nil)
    #expect(!viewer.session.sidebarHidden, "Find shows the hidden sidebar first")
    #expect(viewer.session.search.isOpen && viewer.session.search.focusRequests == 1)
    for place in ViewerPlace.keptPlaces {
      viewer.session.place = place
      #expect(window.validateMenuItem(find), "\(place)")
      window.findInTab(nil)
    }
    #expect(viewer.session.search.focusRequests == 1 + ViewerPlace.keptPlaces.count, "every list searches the sidebar")
  }

  @Test("⌥⌘I is gone and ⌃⌘S hides the sidebar")
  func optionCommandIIsGoneAndControlCommandSHidesTheSidebar() async throws {
    let viewer = ViewerController.scripted(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    let sidebar = item(#selector(ViewerMenuActions.toggleSidebar(_:)))
    #expect(window.validateMenuItem(sidebar))
    #expect(sidebar.title == ViewerWords.hideSidebarItem)
    window.toggleSidebar(nil)
    #expect(viewer.state.sidebar.mode == .hidden)
    #expect(window.validateMenuItem(sidebar))
    #expect(sidebar.title == ViewerWords.showSidebarItem)
    window.toggleSidebar(nil)
    #expect(viewer.state.sidebar.mode == .full)
    let view = MainMenu.view()
    let shortcut = try #require(view.items.first { $0.action == #selector(ViewerMenuActions.toggleSidebar(_:)) })
    #expect(shortcut.keyEquivalent == "s" && shortcut.keyEquivalentModifierMask == [.command, .control])
    let every = MainMenu.make(viewer: true).items
      .compactMap(\.submenu)
      .flatMap(\.items)
    #expect(!every.contains { $0.keyEquivalent == "i" && $0.keyEquivalentModifierMask == [.command, .option] })
  }

  @Test("View's sidebar item shows the sidebar while the window is too narrow for it, over the card")
  func theSidebarItemFollowsTheNarrowWindow() {
    let viewer = ViewerController.scripted(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    viewer.fitWindow(to: CGSize(width: 400, height: 600))
    let sidebar = item(#selector(ViewerMenuActions.toggleSidebar(_:)))
    #expect(window.validateMenuItem(sidebar))
    #expect(sidebar.title == ViewerWords.showSidebarItem)
    window.toggleSidebar(nil)
    #expect(viewer.state.sidebar.overlay)
    #expect(window.validateMenuItem(sidebar))
    #expect(sidebar.title == ViewerWords.hideSidebarItem)
  }

  @Test("the helper line's menu offers Check Again, Open Settings, Recover, Switch and Log Out as the Helper menu does")
  func theHelperLinesMenuFollowsTheHelperMenu() throws {
    let viewer = ViewerController.scripted(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    let pairs: [(ViewerCommand, Selector)] = [
      (.checkAgain, #selector(ViewerMenuActions.checkAgain(_:))),
      (.openSettings, #selector(ViewerMenuActions.openHelperSettings(_:))),
      (.askRecover, #selector(HelperMenuActions.askRecover(_:))),
      (.askSwitch, #selector(HelperMenuActions.askSwitch(_:))),
      (.askLogOut, #selector(HelperMenuActions.askLogOut(_:))),
    ]
    let agree = { (why: String) in
      let line = viewer.state.sidebar.helper.menu.flatMap(\.items)
      #expect(line.map(\.command) == pairs.map(\.0), "\(why): in the Helper menu's order")
      for (command, action) in pairs {
        let lineItem = try #require(line.first { $0.command == command })
        let menuItem = item(action)
        #expect(lineItem.isEnabled == window.validateMenuItem(menuItem), "\(why): \(command)")
        // Switch and Log Out name the helper once it is known, as the Helper menu's validation titles them.
        if command == .askSwitch || command == .askLogOut { #expect(lineItem.title == menuItem.title, "\(why)") }
      }
    }
    try agree("before the first reading")
    viewer.readingLoop.show(
      try status(["overview": ["mode": "observing", "accountName": "Test Helper", "account": "test-helper"]])
    )
    try agree("watching")
    viewer.session.alert = .restart
    try agree("an alert up")
    viewer.session.alert = nil
    viewer.readingLoop.show(try status(["overview": ["mode": "recovering", "pending": 1]]))
    viewer.cancelAlert()
    try agree("needing the person")
    viewer.session.checkingAgain = true
    try agree("checking")
    let helper = MainMenu.helper()
    let check = try #require(helper.items.first { $0.action == #selector(ViewerMenuActions.checkAgain(_:)) })
    #expect(check.keyEquivalent == "r" && check.keyEquivalentModifierMask == .command)
    #expect(viewer.state.sidebar.helper.menu.first?.items.first?.shortcut == ViewerMenuShortcut("r"))
  }

  @Test("Next Session and Previous Session follow the sidebar's order")
  func nextAndPreviousSessionFollowTheSidebar() throws {
    let viewer = ViewerController.scripted(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    viewer.readingLoop.show(
      try status([
        "overview": [
          "mode": "recovering", "state": "needsYou", "pending": 1, "sessionsShown": true,
          "hostSessions": [SessionReplies.cursorIdle, SessionReplies.codexWaiting(), SessionReplies.claudeNeedsYou],
        ]
      ])
    )
    let next = item(#selector(ViewerMenuActions.chooseNextSession(_:)))
    let previous = item(#selector(ViewerMenuActions.choosePreviousSession(_:)))
    let all = item(#selector(ViewerMenuActions.chooseAllSessions(_:)))
    #expect(!window.validateMenuItem(previous) && !window.validateMenuItem(all), "All sessions is first")
    var visited: [ViewerSessionChoice] = []
    while window.validateMenuItem(next) {
      window.chooseNextSession(nil)
      visited.append(viewer.session.chosenSession)
    }
    // Needs you leads, then Working, then Idle, as the sidebar draws them.
    #expect(visited == [.one("3f9a1c07b2e4"), .one("8be20c4d1a77"), .one("5d21e9a04c3f")])
    #expect(visited == Array(viewer.state.sidebar.sessions.choices.dropFirst()))
    window.choosePreviousSession(nil)
    #expect(viewer.session.chosenSession == .one("8be20c4d1a77"))
    #expect(window.validateMenuItem(all))
    window.chooseAllSessions(nil)
    #expect(viewer.session.chosenSession == .all)
    let session = MainMenu.session()
    let keys = session.items
      .filter { $0.action != nil && !$0.keyEquivalent.isEmpty }
      .map { "\($0.keyEquivalent.unicodeScalars.first?.value ?? 0)" }
    #expect(keys == ["48", "\(NSUpArrowFunctionKey)", "\(NSDownArrowFunctionKey)"])
    #expect(
      session.items.filter { !$0.keyEquivalent.isEmpty }.allSatisfy {
        $0.keyEquivalentModifierMask == [.command, .option]
      }
    )
  }

  @Test("Show Ended Sessions is ticked while Ended is open")
  func showEndedSessionsIsTickedWhileEndedIsOpen() throws {
    let viewer = ViewerController.scripted(ScriptedService())
    let window = window(viewer)
    defer { window.close() }
    let ended = item(#selector(ViewerMenuActions.toggleEnded(_:)))
    #expect(window.validateMenuItem(ended))
    #expect(ended.state == .off)
    window.toggleEnded(nil)
    #expect(viewer.session.endedShown)
    #expect(window.validateMenuItem(ended))
    #expect(ended.state == .on)
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
    viewer.session.place = .screenshots
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
