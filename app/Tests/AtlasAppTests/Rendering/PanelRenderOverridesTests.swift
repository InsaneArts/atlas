import AtlasKit
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("PanelRenderOverrides")
struct PanelRenderOverridesTests {
  @Test("with nothing to change, the live panel is drawn as it is")
  func nothingChangesTheLivePanel() {
    let live = PanelRenderOverrides(environment: ["ATLAS_RENDER_MODE": "nonsense", "ATLAS_RENDER_MESSAGE": ""])
      .applied(to: PanelSnapshot())
    #expect(live.status.overview.mode == .unavailable, "an unknown mode keeps the live one")
    #expect(live.feedback == nil)
    #expect(live.logOutStage == .idle)
    #expect(live.helper == nil && live.opensAtLogin == nil)
  }

  @Test("the mode and the message replace the live ones")
  func theModeAndMessageReplaceTheLiveOnes() {
    let shown = PanelRenderOverrides(environment: ["ATLAS_RENDER_MODE": "controlling", "ATLAS_RENDER_MESSAGE": "Hi"])
      .applied(to: PanelSnapshot())
    #expect(shown.status.overview.mode == .controlling)
    #expect(shown.feedback == "Hi")
  }

  @Test("a log-out step shows its stage and every action, as on a set-up Mac", arguments: ["ask", "held"])
  func aLogOutStepShowsEveryAction(step: String) {
    let shown = PanelRenderOverrides(environment: ["ATLAS_RENDER_LOGOUT": step]).applied(to: PanelSnapshot())
    #expect(shown.logOutStage == (step == "ask" ? .confirming : .held))
    #expect(shown.helper != nil && shown.opensAtLogin?.isAvailable == true)
  }

  @Test("the preview flag shows every action with no log-out step")
  func thePreviewShowsEveryAction() {
    let shown = PanelRenderOverrides(environment: ["ATLAS_RENDER_PREVIEW": "1"]).applied(to: PanelSnapshot())
    #expect(shown.helper != nil && shown.opensAtLogin?.isAvailable == true)
    #expect(shown.logOutStage == .idle)
  }
}
