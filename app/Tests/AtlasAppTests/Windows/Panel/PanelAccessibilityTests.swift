import AppKit
import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@MainActor
@Suite("Panel accessibility", .serialized, AppKitState())
struct PanelAccessibilityTests {
  @Test("the panel is announced as Atlas")
  func thePanelIsAnnouncedAsAtlas() {
    let service = ScriptedService()
    let commands = ScriptedCommands([:])
    let clock = ManualClock()
    let controller = PanelController(
      dependencies: PanelDependencies(
        service: service,
        commands: commands,
        actions: HelperActions.scripted(service, commands: commands, clock: clock),
        loginItem: ScriptedLoginItem().seam,
        clock: clock
      )
    )
    // A status bar of its own, which shows in no menu bar.
    let panel = PanelWindow(
      controller: controller,
      item: StatusItem(bar: NSStatusBar()) { false },
      openSettings: {},
      gettingStarted: {}
    )
    // Its heading is the status, not Atlas's name, so the window carries the name itself.
    #expect(controller.state.status.label != "Atlas")
    #expect(panel.title == "Atlas")
    #expect(panel.accessibilityTitle() == "Atlas")
    controller.watch(open: false)
  }
}
