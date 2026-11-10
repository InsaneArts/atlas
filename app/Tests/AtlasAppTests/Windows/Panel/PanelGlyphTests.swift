import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasApp

@Suite("Panel glyph")
@MainActor
struct PanelGlyphTests {
  @Test("while a question waits the menu bar says Waiting for you, whether or not Atlas may notify the person")
  func aWaitingQuestionBadgesTheMenuBar() async throws {
    let service = ScriptedService()
    service.reply = ["overview": ["mode": "agent", "state": "working"]]
    let commands = ScriptedCommands([:])
    let clock = ManualClock()
    let store = await AskNotificationsTests.store(reading: [AskNotificationsTests.ask("a1")])
    let panel = PanelController(
      dependencies: PanelDependencies(
        service: service,
        commands: commands,
        actions: HelperActions(
          console: SessionConsole(read: { true }),
          commands: commands,
          service: service,
          barrier: ControlBarrier(service: service, clock: clock),
          endControl: {},
          asks: store
        ),
        loginItem: ScriptedLoginItem().seam,
        clock: clock,
        wallClock: { AskNotificationsTests.asked }
      )
    )
    await panel.readStatus()
    #expect(panel.glyph.kind == .needsYou)
    #expect(panel.glyph.word == AskWords.waiting)
    await store.answer("a1", with: AskAnswer(.deny))
    #expect(panel.glyph.kind != .needsYou, "answered, the menu bar shows the helper again")
  }
}
