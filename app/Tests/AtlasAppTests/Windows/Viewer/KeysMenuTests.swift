import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("KeysMenu")
struct KeysMenuTests {
  @Test("the Keys menu offers exactly the board's six keys")
  func theKeysMenuOffersTheBoardsSixKeys() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    let menu = KeysMenu.menu(for: viewer)
    let items = menu.items.compactMap { $0 as? HandlerMenuItem }
    #expect(
      items.map(\.title) == ["Switch apps", "Spotlight", "Mission Control", "Close window", "Quit app", "Escape"]
    )
    #expect(items.map(\.keyEquivalentModifierMask) == [.command, .command, .control, .command, .command, []])
    for item in items { item.fire() }
    try await settle("the keys were never all sent") { service.actions.count == items.count }
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
    let items = KeysMenu.menu(for: viewer).items
      .compactMap { $0 as? HandlerMenuItem }
    let switchApps = try #require(items.first { $0.title == "Switch apps" })
    let escape = try #require(items.first { $0.title == "Escape" })
    switchApps.fire()
    // Input leaves in order, so a key sent once in control arriving alone means the first key sent nothing.
    await viewer.takeControl()
    escape.fire()
    try await settle("the key sent in control never arrived") { !service.actions.isEmpty }
    #expect(service.actions == [[InputAction.key("escape")]])
    viewer.close()
  }
}
