import AsyncSupportTesting
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("PanelController announcements")
struct PanelControllerAnnouncementTests {
  @Test("the open panel announces asking states after a second, and stays quiet for Working and Idle")
  func onlyAskingStatesAreAnnounced() async throws {
    let service = ScriptedService()
    let clock = ManualClock()
    let panel = PanelController.scripted(service, clock: clock)
    panel.isOpen = true
    service.reply = ["overview": ["mode": "agent", "state": "working"]]
    await panel.readStatus()
    #expect(panel.askingStatus == nil)
    service.reply = ["overview": ["mode": "observing", "state": "idle"]]
    await panel.readStatus()
    #expect(panel.askingStatus == nil)
    service.reply = ["overview": ["mode": "stopped", "state": "stopped"]]
    await panel.readStatus()
    let text = try #require(panel.askingStatus)
    panel.announceSoon(text)
    try await clock.untilSleeping(for: PanelController.announceAfter, "the announcement never waited")
    clock.advance(by: PanelController.announceAfter - .milliseconds(1))
    await Task.yield()
    #expect(panel.state.announcement == nil)
    clock.advance(by: .milliseconds(1))
    try await settle("the announcement never came") { panel.state.announcement == text }
    panel.announceSoon(text)
    #expect(clock.waiting.isEmpty, "a repeated reading starts no new announcement")
    panel.isOpen = false
    panel.restartAnnouncements()
    #expect(panel.askingStatus == nil)
    #expect(panel.state.announcement == nil)
  }

  @Test("a newer asking state replaces a pending announcement, and closing calls it off")
  func pendingAnnouncementsAreCancelled() async throws {
    let clock = ManualClock()
    let panel = PanelController.scripted(clock: clock)
    panel.isOpen = true
    panel.announceSoon("Stopped")
    try await clock.untilSleeping(for: PanelController.announceAfter, "the first announcement never waited")
    panel.announceSoon("Needs you")
    try await clock.untilSleeping(for: PanelController.announceAfter, "the replacement never waited")
    clock.advance(by: PanelController.announceAfter)
    try await settle("the replacement was never announced") { panel.state.announcement == "Needs you" }
    panel.announceSoon("Logged out")
    try await clock.untilSleeping(for: PanelController.announceAfter, "the last announcement never waited")
    panel.isOpen = false
    panel.restartAnnouncements()
    clock.advance(by: PanelController.announceAfter)
    await Task.yield()
    #expect(panel.state.announcement == nil)
  }
}
