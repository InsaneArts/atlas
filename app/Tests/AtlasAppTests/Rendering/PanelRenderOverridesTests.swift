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

  @Test(
    "each sessions render lists its sessions, one session included, in Recent's place",
    arguments: zip(PanelRender.sessions, [1, 2, 3, 3, 3])
  )
  func eachSessionsRenderListsItsSessions(sessions: String, rows: Int) {
    let mode = sessions == "needsYou" ? "recovering" : "agent"
    let shown = PanelRenderOverrides(
      environment: ["ATLAS_RENDER_MODE": mode, "ATLAS_RENDER_PREVIEW": "1", "ATLAS_RENDER_SESSIONS": sessions]
    )
    .applied(to: PanelSnapshot())
    let panel = PanelPresenter.state(shown)
    #expect(panel.sessions.count == rows)
    #expect(panel.recent.isEmpty)
    #expect((panel.moreSessions != nil) == (sessions == "more"))
  }

  @Test("the service note render shows the note over the live panel, and no other render does")
  func theServiceNoteRenderShowsTheNote() {
    let note = PanelRenderOverrides(environment: ["ATLAS_RENDER_SERVICE_NOTE": "1", "ATLAS_RENDER_PREVIEW": "1"])
      .applied(to: PanelSnapshot())
    #expect(PanelPresenter.state(note).showsServiceNote)
    let plain = PanelRenderOverrides(environment: ["ATLAS_RENDER_PREVIEW": "1"])
      .applied(to: PanelSnapshot())
    #expect(!PanelPresenter.state(plain).showsServiceNote)
  }

  @Test("the idle note render names its app, and no other render shows it")
  func theIdleNoteRenderShowsTheNote() {
    let quit = PanelRenderOverrides(environment: ["ATLAS_RENDER_IDLE_NOTE": "Terminal", "ATLAS_RENDER_PREVIEW": "1"])
      .applied(to: PanelSnapshot())
    #expect(PanelPresenter.state(quit).idleNote == PanelIdleFix(app: "Terminal"))
    let plain = PanelRenderOverrides(environment: ["ATLAS_RENDER_PREVIEW": "1"])
      .applied(to: PanelSnapshot())
    #expect(PanelPresenter.state(plain).idleNote == nil)
  }
}
