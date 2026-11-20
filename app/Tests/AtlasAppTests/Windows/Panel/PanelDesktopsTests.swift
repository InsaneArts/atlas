import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("Panel desktops")
@MainActor
struct PanelDesktopsTests {
  /// Second Login's reply while nothing happens there.
  static let idle: [String: Any] = [
    "ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"],
  ]
  /// work's reply while an agent acts there, on its 1920 by 1200 screen.
  static let working: [String: Any] = [
    "ready": true, "canObserve": true,
    "overview": ["mode": "agent", "state": "working", "display": ["width": 1920, "height": 1200]],
  ]

  /// A panel and a main window sharing one set of the helper's commands, over `service`, as the app has them.
  ///
  /// - Parameter service: The scripted service.
  /// - Returns: The panel and the window.
  static func both(_ service: ScriptedService) -> (PanelController, ViewerController) {
    let commands = ScriptedCommands([:])
    let clock = ManualClock()
    let viewer = WeakViewer()
    let actions = HelperActions.scripted(service, commands: commands, clock: clock) {
      viewer.controller?.control.releaseControl()
    }
    let panel = PanelController(
      dependencies: PanelDependencies(
        service: service,
        commands: commands,
        actions: actions,
        loginItem: ScriptedLoginItem().seam,
        clock: clock
      )
    )
    let window = ViewerController.scripted(service, clock: clock, actions: actions)
    viewer.controller = window
    return (panel, window)
  }

  @Test("with one desktop nothing changes: no popup, the same panel, and the menu bar shows its own mode")
  func withOneDesktopNothingChanges() async throws {
    let service = ScriptedService()
    service.reply = Self.working
    let panel = PanelController.scripted(service)
    await panel.readStatus()
    let before = panel.state
    let glyph = panel.glyph
    service.desktopList = DesktopLists.one
    await panel.readStatus()
    #expect(panel.actions.desktops.list == DesktopLists.one)
    #expect(panel.state.desktop == nil)
    #expect(panel.state == before)
    #expect(panel.glyph == glyph)
    #expect(service.log.isEmpty, "reading the desktops changes nothing")
  }

  @Test("picking a desktop changes only the viewed one, and the panel then names it")
  func pickingADesktopChangesOnlyTheViewedOne() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    service.desktopList = DesktopLists.two(shown: "second-login")
    let panel = PanelController.scripted(service)
    await panel.readStatus()
    #expect(panel.state.desktop?.title == "Second Login")
    #expect(panel.state.desktop?.items.map(\.title) == ["Second Login", "work"])
    service.reply = Self.working
    panel.send(.view(desktop: "work"))
    try await settle("work was never shown") {
      panel.state.desktop?.title == "work" && panel.status.overview.state == .working
    }
    #expect(service.log == ["view work"], "nothing but the desktop shown changes")
    let list = try #require(panel.actions.desktops.list)
    #expect(list.desktops.map(\.isDefault) == [true, false], "Second Login stays the default")
    #expect(panel.actions.desktops.moves == 1)
  }

  @Test("a reply for another desktop after a switch is dropped, and a refused one is no failure")
  func aReplyForAnotherDesktopIsDropped() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    service.desktopList = DesktopLists.two(shown: "second-login")
    let panel = PanelController.scripted(service)
    await panel.readStatus()
    service.hold = ["status"]
    let late = Task { await panel.readStatus() }
    try await settle("Second Login's reading never started") { service.held["status"] != nil }
    service.reply = Self.working
    await panel.view("work")
    #expect(panel.status.overview.state == .working)
    service.resume("status")
    await late.value
    #expect(panel.status.overview.state == .working, "Second Login's late reply never shows under work")
    service.refusals["status"] = .otherDesktop
    await panel.readStatus()
    #expect(panel.answered == true, "a reply dropped for its desktop is not the service failing")
    #expect(panel.status.overview.state == .working)
  }

  @Test("a view changed elsewhere is followed: the panel and the window forget the desktop shown before")
  func aViewChangedElsewhereIsFollowed() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    service.picture = png()
    service.desktopList = DesktopLists.two(shown: "second-login")
    let (panel, viewer) = Self.both(service)
    await panel.readStatus()
    await panel.readFrame()
    #expect(panel.picture != nil)
    viewer.setVisible(true)
    try await settle("the window never showed Second Login") { viewer.reading.frame != nil }
    service.desktopList = DesktopLists.two(shown: "work")
    service.reply = Self.working
    await panel.readStatus()
    #expect(panel.actions.desktops.moves == 1)
    #expect(panel.picture == nil, "Second Login's picture never shows under work")
    #expect(panel.state.desktop?.title == "work")
    viewer.followDesktops()
    #expect(viewer.reading.frame == nil, "the window lets Second Login's picture go")
    #expect(viewer.reading.picture == nil)
    #expect(viewer.state.sidebar.helper.desktops.first(where: \.isViewed)?.id == "work")
    try await settle("work's picture never came") { viewer.reading.frame != nil }
    #expect(viewer.state.sidebar.helper.line == "VM work · 1920 × 1200")
    viewer.close()
  }

  @Test("the menu bar dot shows the most urgent desktop and names it")
  func theMenuBarDotShowsTheMostUrgentDesktop() async {
    let service = ScriptedService()
    service.reply = Self.idle
    service.desktopList = DesktopLists.two(shown: "second-login", work: DesktopLists.work(.needsYou))
    let panel = PanelController.scripted(service)
    await panel.readStatus()
    #expect(panel.glyph == PanelGlyph(kind: .needsYou, word: "Needs you on work"))
    #expect(panel.state.status.label == "Idle", "the panel itself still says the desktop it shows")
    service.desktopList = DesktopLists.two(shown: "second-login", work: DesktopLists.work(.idle))
    await panel.readStatus()
    #expect(panel.glyph == PanelGlyph(kind: .idle, word: "Idle"))
  }

  @Test("the panel's menu shows each desktop's dot and state, like the window's")
  func thePanelMenuShowsDotsAndStates() async throws {
    let service = ScriptedService()
    service.reply = Self.idle
    service.desktopList = DesktopLists.two(shown: "second-login")
    let panel = PanelController.scripted(service)
    await panel.readStatus()
    let picker = try #require(panel.state.desktop)
    #expect(
      picker.items == [
        DesktopItem(id: "second-login", title: "Second Login", word: "Idle", tone: .quiet, isViewed: true),
        DesktopItem(id: "work", title: "work", word: "Working", tone: .active, mark: .pulsing),
      ]
    )
    let menu = DesktopMenu.menu(picker.items) { _ in }
    if #available(macOS 14.4, *) {
      #expect(menu.items.map(\.subtitle) == ["Idle", "Working"])
    }
    #expect(menu.items.allSatisfy { $0.image != nil })
    panel.setDesktopsOpen(true)
    #expect(panel.state.desktop?.isOpen == true, "the popup draws pressed while its menu is open")
  }

  @Test("a Stop names an uncertain desktop by its title in the panel's words")
  func aStopNamesAnUncertainDesktop() async {
    let service = ScriptedService()
    service.reply = Self.working
    service.desktopList = DesktopLists.two(shown: "second-login")
    service.stopOutcome = HelperWordsStopTests.outcome(stopped: ["second-login"], unsure: ["work"])
    let panel = PanelController.scripted(service)
    await panel.readStatus()
    await panel.stopAgent()
    #expect(panel.session.notice == "Agents stopped on Second Login, but Atlas isn't sure they stopped on work.")
  }
}
