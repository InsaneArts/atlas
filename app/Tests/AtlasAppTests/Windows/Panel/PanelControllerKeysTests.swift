import AsyncSupportTesting
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("PanelController shortcuts")
struct PanelControllerKeysTests {
  @Test("the panel's agent shortcut stops and resumes only while its button can be pressed")
  func theAgentShortcutFollowsItsButton() async throws {
    let service = ScriptedService()
    let panel = PanelController.scripted(service)
    #expect(!panel.press(.stopOrResume))
    service.reply = ["overview": ["mode": "agent", "state": "working"]]
    await panel.readStatus()
    #expect(panel.state.controls.agentHelp == "Stop Agent (⌘.)")
    #expect(panel.press(.stopOrResume))
    try await settle("the shortcut never stopped the agent") { service.log.contains("stop") }
    try await settle("the stop never read the status") { service.statusCalls == 2 }
    service.reply = ["overview": ["mode": "stopped", "state": "stopped"]]
    await panel.readStatus()
    try await settle("the stop request never ended") { panel.session.requests == 0 }
    #expect(panel.state.controls.agentHelp == "Resume Agent (⌘.)")
    panel.session.requests = 1
    #expect(!panel.press(.stopOrResume), "a disabled Resume is not pressed")
    panel.session.requests = 0
    #expect(panel.press(.stopOrResume))
    try await settle("the shortcut never resumed the agent") { service.log.contains("resume") }
  }

  @Test("the panel's picture shortcut pauses and resumes only a picture on screen")
  func thePictureShortcutFollowsItsButton() async {
    let service = ScriptedService()
    let panel = PanelController.scripted(service)
    #expect(!panel.press(.pausePicture))
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "observing", "state": "idle"]]
    await panel.readStatus()
    panel.picture = SampleDesktop.picture
    #expect(panel.state.picture.pause.help == "Pause Picture (⇧⌘P)")
    #expect(panel.press(.pausePicture))
    #expect(panel.session.paused)
    #expect(panel.state.picture.pause.help == "Resume Picture (⇧⌘P)")
    #expect(panel.press(.pausePicture))
    #expect(!panel.session.paused)
    panel.watch(open: false)
  }
}
