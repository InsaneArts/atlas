import AppKit
import Testing

@testable import AtlasApp

@Suite("PanelMoreMenu")
struct PanelMoreMenuTests {
  /// Answers the helper's items; none is chosen here.
  final class NoHelper: NSObject, HelperMenuActions {
    func askRecover(_ sender: Any?) {}
    func askRestart(_ sender: Any?) {}
    func askSwitch(_ sender: Any?) {}
    func askLogOut(_ sender: Any?) {}
  }

  @Test("the panel's ⋯ Settings… (⌘,) opens Settings")
  func settingsOpensSettings() throws {
    let opened = CallCount()
    let helper = NoHelper()
    let menu = PanelMoreMenu.menu(
      PanelMoreMenuState(
        helper: HelperMenuState(canRecover: true, canRestart: true, canSwitch: true, canLogOut: true),
        opensAtLogin: nil
      ),
      target: helper,
      actions: PanelMoreMenu.Actions(
        openAtlas: {},
        openSettings: { opened.count += 1 },
        setOpenAtLogin: { _ in },
        quit: {}
      )
    )
    let settings = try #require(menu.items.first { $0.title == MenuWords.settings })
    #expect(settings.keyEquivalent == ",")
    // The item's own target and action, as the menu sends them: a test process has no running app to route
    // `performActionForItem` through.
    let target = try #require(settings.target as? NSObject)
    let action = try #require(settings.action)
    _ = target.perform(action, with: settings)
    #expect(opened.count == 1)
  }
}
