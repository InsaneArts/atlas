import AppKit
import AtlasService
import AtlasStatus
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("Desktop items")
@MainActor
struct DesktopItemsTests {
  /// Working, as the window sees the desktop shown while an agent acts there.
  static let working = mode(.working)

  /// The mode the window sees on the desktop shown while its service says `state`.
  ///
  /// - Parameter state: The overview's state.
  /// - Returns: The mode.
  static func mode(_ state: OverviewState) -> Mode {
    var overview = ViewerStatus.Overview()
    overview.state = state
    return Mode.of(overview)
  }

  @Test("with one desktop nothing changes: no items, no title, and the menu bar shows the desktop's own mode")
  func withOneDesktopNothingChanges() {
    for list in [nil, DesktopList(), DesktopLists.one] {
      #expect(DesktopItems.items(list, shown: Self.working).isEmpty)
      #expect(DesktopItems.shownTitle(list) == nil)
      #expect(DesktopItems.shown(list) == nil)
      let urgent = DesktopItems.mostUrgent(list, shown: Self.working)
      #expect(urgent == DesktopItems.Urgent(kind: .working, word: "Working"))
    }
  }

  @Test("each desktop is listed by its title with its dot and word, the one shown by the window's own mode")
  func eachDesktopIsListed() {
    let items = DesktopItems.items(DesktopLists.two(shown: "work"), shown: Self.working)
    #expect(
      items == [
        DesktopItem(id: "second-login", title: "Second Login", word: "Idle", tone: .quiet),
        DesktopItem(id: "work", title: "work", word: "Working", tone: .active, mark: .pulsing, isViewed: true),
      ]
    )
    let maintained = DesktopLists.two(shown: "second-login", work: DesktopLists.work(.notReady, maintenance: "on"))
    let shownIdle = Self.mode(.idle)
    #expect(
      DesktopItems.items(maintained, shown: shownIdle)
        .last == DesktopItem(id: "work", title: "work", word: "In maintenance", tone: .link),
      "maintenance is said in blue"
    )
    let stopped = DesktopLists.two(shown: "second-login", work: DesktopLists.work(.notReady, power: "stopped"))
    #expect(DesktopItems.items(stopped, shown: shownIdle).last?.word == "Stopped")
  }

  @Test("the menu bar dot shows the most urgent desktop, naming it when it is not the one shown")
  func theMenuBarDotShowsTheMostUrgentDesktop() {
    let idle = Self.mode(.idle)
    let needing = DesktopLists.two(shown: "second-login", work: DesktopLists.work(.needsYou))
    let urgent = DesktopItems.mostUrgent(needing, shown: idle)
    #expect(urgent == DesktopItems.Urgent(kind: .needsYou, word: "Needs you on work"))
    let busy = DesktopLists.two(shown: "second-login")
    #expect(DesktopItems.mostUrgent(busy, shown: idle).kind == .working, "work's agent shows over an idle Second Login")
    let stoppedHere = Self.mode(.stopped)
    #expect(
      DesktopItems.mostUrgent(busy, shown: stoppedHere) == DesktopItems.Urgent(kind: .stopped, word: "Stopped"),
      "the one shown keeps its own word while it is the most urgent"
    )
    let maintained = DesktopLists.two(shown: "second-login", work: DesktopLists.work(.notReady, maintenance: "on"))
    #expect(DesktopItems.mostUrgent(maintained, shown: idle).kind == .idle, "a VM in maintenance asks nothing")
    #expect(
      DesktopItems.mostUrgent(needing, shown: .connecting).kind == .connecting,
      "nothing else counts while the service is not answering"
    )
  }

  @Test("the desktops' menu shows each desktop's dot and state, the one shown checked, then Desktop Settings…")
  func theMenuShowsDotsAndStates() throws {
    let picked = ChoiceCount()
    let settings = ChoiceCount()
    let items = DesktopItems.items(DesktopLists.two(shown: "work"), shown: Self.working)
    let menu = DesktopMenu.menu(items, view: { picked.add($0) }, openSettings: { settings.add("settings") })
    if #available(macOS 14.4, *) {
      #expect(menu.items.map(\.title) == ["Second Login", "work", "", DesktopMenu.settingsItem])
      #expect(menu.items.prefix(2).map(\.subtitle) == ["Idle", "Working"])
    }
    #expect(menu.items.prefix(2).map(\.state) == [.off, .on])
    for item in menu.items.prefix(2) {
      let image = try #require(item.image, "\(item.title) has its dot")
      #expect(!image.isTemplate, "\(item.title)'s dot keeps its colour")
    }
    let panel = DesktopMenu.menu(items, view: { picked.add($0) })
    #expect(panel.items.map(\.title) == ["Second Login", "work"], "the panel's menu lists the desktops alone")
    try fire(menu.items[1])
    #expect(picked.titles.isEmpty, "picking the desktop shown does nothing")
    try fire(menu.items[0])
    #expect(picked.titles == ["second-login"])
    try fire(menu.items[3])
    #expect(settings.titles == ["settings"])
  }

  /// Chooses `item`, as the menu would.
  ///
  /// - Parameter item: A desktop's item, or Desktop Settings….
  /// - Throws: When it is not one that runs a closure.
  private func fire(_ item: NSMenuItem) throws {
    let handler = try #require(item as? HandlerMenuItem)
    handler.fire()
  }
}
