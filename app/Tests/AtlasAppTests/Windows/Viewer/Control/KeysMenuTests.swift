import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("KeysMenu")
struct KeysMenuTests {
  @Test("Send Key offers exactly six keys in title case with their keys in words, under Mac and App Shortcuts")
  func sendKeyOffersTheSixKeys() async throws {
    let menu = KeysMenu.menu()
    let items = menu.items.filter { $0.action == #selector(ViewerMenuActions.sendKey(_:)) }
    #expect(
      items.map(\.title) == [
        "Switch Apps (⌘⇥)", "Spotlight (⌘Space)", "Mission Control (⌃↑)", "Close Window (⌘W)", "Quit App (⌘Q)",
        "Escape (⎋)",
      ]
    )
    #expect(items.map(\.tag) == Array(HelperKey.all.indices))
    #expect(items.allSatisfy { $0.keyEquivalent.isEmpty }, "no key becomes this app's shortcut, ⌘Q and ⌘W among them")
    #expect(menu.items.filter(\.isSectionHeader).map(\.title) == ["Mac Shortcuts", "App Shortcuts"])
  }

  @Test("each key goes to the helper as the service names it, while in control")
  func eachKeyGoesToTheHelper() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.control.takeControl()
    for index in HelperKey.all.indices { viewer.sendKey(at: index) }
    try await settle("the keys were never all sent") { service.actions.count == HelperKey.all.count }
    #expect(
      service.actions
        == ["cmd+tab", "cmd+space", "ctrl+up", "cmd+w", "cmd+q", "escape"].map { [InputAction.key($0)] }
    )
    viewer.close()
  }

  @Test("a key chosen without control sends nothing")
  func nothingIsSentWithoutControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    let switchApps = try #require(HelperKey.all.firstIndex { $0.name == "Switch Apps" })
    let escape = try #require(HelperKey.all.firstIndex { $0.name == "Escape" })
    viewer.sendKey(at: switchApps)
    // Input leaves in order, so a key sent once in control arriving alone means the first key sent nothing.
    await viewer.control.takeControl()
    viewer.sendKey(at: escape)
    try await settle("the key sent in control never arrived") { !service.actions.isEmpty }
    #expect(service.actions == [[InputAction.key("escape")]])
    viewer.close()
  }
}
