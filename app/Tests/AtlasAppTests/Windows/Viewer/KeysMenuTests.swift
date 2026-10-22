import AppKit
import Testing

@testable import AtlasApp
@testable import AtlasKit

@MainActor
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
    let items = KeysMenu.menu(for: viewer).items.compactMap { $0 as? HandlerMenuItem }
    let switchApps = try #require(items.first { $0.title == "Switch apps" })
    switchApps.fire()
    // Input is queued as it is sent and leaves the queue only later, so an empty queue now means nothing was sent.
    #expect(viewer.input.pending.isEmpty)
    viewer.close()
  }
}
