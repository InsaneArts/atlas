import AtlasKit
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("PanelRenderOverrides")
struct PanelRenderOverridesTests {
  @Test("with nothing to change, the live panel is drawn as it is")
  func nothingChangesTheLivePanel() {
    var observing = PanelSnapshot()
    observing.status.overview.mode = .observing
    let live = PanelRenderOverrides(environment: ["ATLAS_RENDER_MODE": "nonsense", "ATLAS_RENDER_MESSAGE": ""])
      .applied(to: observing)
    #expect(live.status.overview.mode == .observing, "an unknown mode keeps the live one")
    #expect(live.session.notice == nil)
    #expect(live.opensAtLogin == nil && live.status.overview.activity.isEmpty)
  }

  @Test(
    "each render mode reads as its state of the panel",
    arguments: zip(
      PanelRender.modes,
      [
        "Idle", "Working", "You have control", "Needs you", "Logged out", "Not ready", "Stopped", "Connecting",
        "In use elsewhere",
      ]
    )
  )
  func eachModeReadsAsItsState(mode: String, label: String) {
    let shown = PanelRenderOverrides(environment: ["ATLAS_RENDER_MODE": mode, "ATLAS_RENDER_PREVIEW": "1"])
      .applied(to: PanelSnapshot())
    #expect(PanelPresenter.state(shown).status.label == label)
  }
}
