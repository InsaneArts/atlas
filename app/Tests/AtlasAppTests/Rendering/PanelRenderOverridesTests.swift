import AtlasUI
import Testing

@testable import AtlasApp

@Suite("PanelRenderOverrides")
struct PanelRenderOverridesTests {
  @Test("the wrong account render names the account even after dismissal")
  func theWrongAccountRenderNamesTheAccount() {
    let snapshot = PanelSnapshot(wrongAccountDismissed: true)
    let shown = PanelRenderOverrides(environment: ["ATLAS_RENDER_WRONG_ACCOUNT": "atlas"])
      .applied(to: snapshot)
    #expect(PanelPresenter.state(shown).wrongAccount == "atlas")
    for environment in [[:], ["ATLAS_RENDER_WRONG_ACCOUNT": ""]] {
      let plain = PanelRenderOverrides(environment: environment)
        .applied(to: PanelSnapshot())
      #expect(PanelPresenter.state(plain).wrongAccount == nil)
    }
  }

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
}
