#if DEBUG
  import AppKit
  import AtlasService
  import AtlasServiceTesting
  import AtlasUI
  import Foundation
  import Testing

  @testable import AtlasApp

  @Suite("Words while a VM is shown")
  @MainActor
  struct VMShownWordsTests {
    /// The desktops with work, a VM, shown.
    static let vm = DesktopLists.two(shown: "work")

    /// Every string `value` holds, however deep: what a window draws, says to VoiceOver and lists in its menus.
    ///
    /// - Parameter value: A window's state, or any part of it.
    /// - Returns: The strings, in no set order.
    static func strings(_ value: Any) -> [String] {
      if let text = value as? String { return [text] }
      return Mirror(reflecting: value)
        .children
        .flatMap { strings($0.value) }
    }

    /// The strings of `value` that say "helper", whatever its case.
    ///
    /// - Parameter value: A window's state, or any part of it.
    /// - Returns: The strings.
    static func helperWords(_ value: Any) -> [String] {
      Set(strings(value))
        .filter { $0.localizedCaseInsensitiveContains("helper") }
        .sorted()
    }

    /// The main window in each state a VM can be in while shown, sessions and Activity included.
    ///
    /// - Returns: Each state by its name.
    static func viewerStates() throws -> [String: ViewerState] {
      let scenes = ViewerPreviewStatesTests()
      let checks: [[String: Any]] = [["name": "Input service ready", "ok": true]]
      let reported = { (mode: String, state: String) in
        try scenes.reported(mode, state: state, checks: checks, reason: "work isn't ready yet")
      }
      let onVM = { (snapshot: inout ViewerSnapshot) in snapshot.desktops = Self.vm }
      let sessions = try scenes.sessionsReported(
        "agent",
        state: "working",
        [SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle]
      )
      let needing = try scenes.sessionsReported(
        "recovering",
        state: "needsYou",
        [SessionReplies.claudeNeedsYou, SessionReplies.codexWaiting(recovery: true)]
      )
      var states: [String: ViewerState] = [:]
      for (name, mode, state) in [
        ("working", "agent", "working"), ("idle", "observing", "idle"), ("stopped", "stopped", "stopped"),
        ("needs you", "recovering", "needsYou"), ("not ready", "unavailable", "notReady"),
        ("in use elsewhere", "controlling", "control"),
      ] {
        states[name] = scenes.shown(try reported(mode, state), onVM)
      }
      states["control"] = scenes.shown(try reported("controlling", "control")) {
        onVM(&$0)
        $0.lease = "lease"
      }
      states["paused"] = scenes.shown(try reported("observing", "idle")) {
        onVM(&$0)
        $0.session.paused = true
      }
      states["paused before a picture"] = scenes.shown(try reported("observing", "idle"), picture: false) {
        onVM(&$0)
        $0.session.paused = true
      }
      states["not ready, waiting"] = scenes.shown(try reported("unavailable", "notReady"), picture: false) {
        onVM(&$0)
        $0.reading.status.overview.reason = nil
      }
      states["no picture"] = scenes.shown(try reported("unavailable", "notReady"), picture: false, unavailable: "x") {
        onVM(&$0)
      }
      states["connecting"] = scenes.shown(ViewerStatus(), picture: false) {
        onVM(&$0)
        $0.reading.hasStatus = false
      }
      states["sessions"] = scenes.shown(sessions, onVM)
      states["sessions need you"] = scenes.shown(needing, onVM)
      states["activity"] = scenes.shown(try reported("observing", "idle")) {
        onVM(&$0)
        $0.session.place = .activity
      }
      states["activity, nothing yet"] = scenes.shown(
        try scenes.reported("observing", state: "idle", activity: [], checks: checks, reason: nil)
      ) {
        onVM(&$0)
        ViewerPreviewStatesTests.onPlace(.activity, at: ViewerPreviewStatesTests.activityReadAt, &$0)
        $0.panes.activity.log = ActivityLog()
        $0.panes.activity.readAt = ViewerPreviewStatesTests.activityReadAt
      }
      states["recover alert"] = scenes.shown(try reported("recovering", "needsYou")) {
        onVM(&$0)
        $0.session.alert = .helper(.recover)
      }
      return states
    }

    /// The panel in each state a VM can be in while shown, sessions and a Stop's words included.
    ///
    /// - Returns: Each state by its name.
    static func panelStates() throws -> [String: PanelState] {
      let scenes = PanelPreviewStatesTests()
      var snapshots: [String: PanelSnapshot] = [:]
      for (name, mode, state) in [
        ("working", "agent", "working"), ("idle", "observing", "idle"), ("stopped", "stopped", "stopped"),
        ("needs you", "recovering", "needsYou"), ("not ready", "unavailable", "notReady"),
        ("in use elsewhere", "controlling", "control"),
      ] {
        snapshots[name] = try scenes.snapshot(mode, state: state, desktops: Self.vm)
      }
      var paused = try scenes.snapshot("observing", state: "idle", desktops: Self.vm)
      paused.session.paused = true
      snapshots["paused"] = paused
      var connecting = PanelSnapshot(connecting: true)
      connecting.desktops = Self.vm
      snapshots["connecting"] = connecting
      var waiting = try SessionReplies.snapshot([
        SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
      ])
      waiting.desktops = Self.vm
      snapshots["sessions"] = waiting
      var holding = try SessionReplies.snapshot([SessionReplies.claudeActing])
      holding.desktops = Self.vm
      snapshots["one session"] = holding
      var jobsLeft = try scenes.snapshot("stopped", state: "stopped", desktops: Self.vm)
      jobsLeft.session.notice = HelperWords.stopped(
        StopOutcome(warnings: [], refused: [StopOutcome.Job(id: "7c2e91a04b3f")]),
        noun: jobsLeft.noun
      )
      snapshots["jobs left"] = jobsLeft
      // The service words a VM's own actions, such as its recoveries; the fixtures' are Second Login's.
      return snapshots.mapValues { snapshot in
        var shown = snapshot
        shown.status.overview.activity = shown.status.overview.activity.filter { $0.operation != "restart" }
        return PanelPresenter.state(shown)
      }
    }

    @Test("with a VM shown, no word of the panel, its menus or the alert window says helper")
    func thePanelNeverSaysHelperForAVM() throws {
      for (name, state) in try Self.panelStates() {
        let words = Self.helperWords(state)
        #expect(words.isEmpty, "\(name): \(words)")
      }
      let more = PanelMoreMenu.menu(
        PanelMoreMenuState(showsOpenAtlas: true),
        actions: PanelMoreMenu.Actions(openAtlas: {}, openSettings: {}, gettingStarted: {}, quit: {})
      )
      #expect(Self.helperWords(more.items.map(\.title)).isEmpty)
      let items = DesktopItems.items(Self.vm, shown: DesktopItemsTests.working)
      let desktops = DesktopMenu.menu(items, view: { _ in }, openSettings: {})
      #expect(Self.helperWords(desktops.items.map(\.title)).isEmpty)
      var overview = ViewerStatus.Overview()
      overview.mode = .recovering
      for alert in [AlertSnapshot(alert: .recover, overview: overview, noun: .vm("work"))] {
        let words = Self.helperWords(AlertPresenter.state(alert))
        #expect(words.isEmpty, "\(words)")
      }
    }

    @Test("with a VM shown, the main menu's Helper menu reads Desktop, and back to Helper for Second Login")
    func theMainMenuNamesTheDesktop() throws {
      let main = NSMenu()
      let item = NSMenuItem(title: MenuWords.helper, action: nil, keyEquivalent: "")
      item.submenu = MainMenu.helper()
      main.addItem(item)
      MainMenu.name(.vm("work"), in: main)
      #expect(item.title == "Desktop")
      let noun = DesktopNoun.vm("work")
      #expect(Self.helperWords([item.title, noun.settingsItem, noun.restartItem, noun.restartTitle]).isEmpty)
      MainMenu.name(.helper, in: main)
      #expect(item.title == MenuWords.helper)
    }

    @Test("with a VM shown, no word or menu item of the main window says helper, session lines included")
    func theMainWindowNeverSaysHelperForAVM() throws {
      let states = try Self.viewerStates()
      let empty = try #require(states["activity, nothing yet"])
      #expect(Self.strings(empty).contains("What agents and you do in work shows up here."))
      let sessions = try #require(states["sessions"])
      #expect(Self.strings(sessions).contains("Waiting for work · 8 s"))
      for (name, state) in states {
        let words = Self.helperWords(state)
        #expect(words.isEmpty, "\(name): \(words)")
      }
      #expect(!Self.helperWords(PreviewStates.viewerSessions).isEmpty, "Second Login's own words still say helper")
    }
  }
#endif
