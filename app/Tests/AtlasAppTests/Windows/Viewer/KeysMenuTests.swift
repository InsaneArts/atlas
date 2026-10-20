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
    #expect(items.count == HelperKey.groups.joined().count)
    let switchApp = try #require(items.first { $0.title == "Switch App" })
    #expect(switchApp.keyEquivalent == "\t" && switchApp.keyEquivalentModifierMask == .command)
    switchApp.fire()
    try #require(items.first { $0.title == "Escape" }).fire()
    try await settle("the two keys were never sent") { service.actions.count == 2 }
    #expect(service.actions == [[.key("cmd+tab")], [.key("escape")]])
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
