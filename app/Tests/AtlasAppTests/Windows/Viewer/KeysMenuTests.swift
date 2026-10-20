import AppKit
import Testing

@testable import AtlasApp
@testable import AtlasKit

@MainActor
@Suite("KeysMenu")
struct KeysMenuTests {
  @Test("each key shows its key equivalent and sends the key it names")
  func theKeysMenuSendsTheKeyItNames() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    let menu = KeysMenu.menu(for: viewer)
    let items = menu.items.compactMap { $0 as? KeyItem }
    let keys = Array(HelperKey.groups.joined())
    #expect(items.map(\.title) == keys.map(\.name))
    #expect(items.map(\.keyEquivalent) == keys.map(\.shown))
    #expect(items.map(\.keyEquivalentModifierMask) == keys.map(\.modifiers))
    let switchApp = try #require(items.first { $0.title == "Switch App" })
    #expect(switchApp.keyEquivalent == "\t" && switchApp.keyEquivalentModifierMask == .command)
    for item in items { item.fire() }
    try await settle("the keys were never all sent") { service.actions.count == keys.count }
    #expect(service.actions == keys.map { [.key($0.sent)] })
    #expect(service.actions.first == [.key("escape")] && service.actions.last == [.key("cmd+q")])
    viewer.close()
  }

  @Test("a key chosen without control sends nothing")
  func nothingIsSentWithoutControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    let menu = KeysMenu.menu(for: viewer)
    let items = menu.items.compactMap { $0 as? KeyItem }
    try #require(items.first { $0.title == "Switch App" }).fire()
    // Input is queued as it is sent and leaves the queue only later, so an empty queue now means nothing was sent.
    #expect(viewer.input.pending.isEmpty)
    viewer.close()
  }
}
