import AppKit
import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@MainActor
@Suite("Panel accessibility")
struct PanelAccessibilityTests {
  @Test("the panel is announced as Atlas")
  func thePanelIsAnnouncedAsAtlas() throws {
    let service = ScriptedService()
    let commands = ScriptedCommands([:])
    let clock = ManualClock()
    let controller = PanelController(
      dependencies: PanelDependencies(
        service: service,
        commands: commands,
        actions: HelperActions(
          commands: commands,
          service: service,
          barrier: ControlBarrier(service: service, clock: clock)
        ),
        loginItem: ScriptedLoginItem().seam,
        clock: clock
      )
    )
    // A status bar of its own, which shows in no menu bar.
    let panel = PanelPopover(
      controller: controller,
      item: StatusItem(bar: NSStatusBar()) { false },
      openSettings: {},
      gettingStarted: {}
    )
    // Its heading is the status, not Atlas's name, so the popover carries the name itself.
    #expect(controller.state.status.label != "Atlas")
    let content = try #require(panel.popover.contentViewController?.view)
    #expect(content.isAccessibilityElement())
    #expect(content.accessibilityLabel() == "Atlas")
    controller.watch(open: false)
  }
}
