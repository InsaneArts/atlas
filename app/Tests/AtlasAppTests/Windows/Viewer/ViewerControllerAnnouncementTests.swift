import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("ViewerController announcements")
struct ViewerControllerAnnouncementTests {
  @Test("VoiceOver hears only a state that asks something of the person, once it has held for a second")
  func onlyAskingStatesAreAnnounced() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent", "state": "working"]]
    let clock = ManualClock()
    let viewer = try await ViewerController.watching(service, clock: clock)
    #expect(viewer.askingStatus == nil, "Working asks nothing of the person")
    viewer.announceSoon("Stopped, Resume when you're ready")
    try await clock.untilSleeping(for: ViewerController.announceAfter, "the announcement never waited")
    clock.advance(by: ViewerController.announceAfter - .milliseconds(1))
    await Task.yield()
    #expect(viewer.state.announcement == nil, "it waits its second")
    clock.advance(by: .milliseconds(1))
    try await settle("the announcement never came") { viewer.state.announcement != nil }
    #expect(viewer.state.announcement == "Stopped, Resume when you're ready")
    viewer.announceSoon(nil)
    #expect(viewer.state.announcement == nil, "a state that asks nothing calls it off at once")
    viewer.close()
  }

  @Test("a state that asks something is said only by the window that has the keyboard")
  func onlyTheKeyWindowAnnounces() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "stopped", "state": "stopped"]]
    let viewer = try await ViewerController.watching(service)
    try await settle("the stop never read") { viewer.snapshot.mode.kind == .stopped }
    #expect(viewer.askingStatus?.hasPrefix("Stopped") == true)
    viewer.control.lostFocus()
    #expect(viewer.askingStatus == nil)
    viewer.close()
  }
}
