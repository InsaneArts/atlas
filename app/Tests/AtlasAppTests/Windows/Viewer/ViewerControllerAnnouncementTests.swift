import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Testing

@testable import AtlasApp

@Suite("ViewerController announcements", .serialized, AppKitState())
struct ViewerControllerAnnouncementTests {
  @Test("a normally constructed viewer subscribes while hidden, then announces after presentation and focus changes")
  func aWindowSubscribesBeforeItShows() async throws {
    _ = NSApplication.shared
    let service = ScriptedService()
    let clock = ManualClock()
    let viewer = ViewerController.scripted(service, clock: clock)
    viewer.session.paused = true
    let window = ViewerWindow(controller: viewer) { _ in }
    defer { window.close() }
    #expect(!viewer.shown)
    window.present()
    // Occlusion and key notifications may arrive later in a test runner; deliver the visible lifecycle explicitly.
    viewer.setVisible(true)
    window.windowDidBecomeKey(Notification(name: NSWindow.didBecomeKeyNotification, object: window))
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "stopped", "state": "stopped"]]
    await viewer.readingLoop.readStatus()
    try await clock.untilSleeping(for: ViewerController.announceAfter, "the window never subscribed")
    clock.advance(by: ViewerController.announceAfter)
    try await settle("the window never announced") { viewer.session.announcement?.hasPrefix("Stopped") == true }
    window.windowDidResignKey(Notification(name: NSWindow.didResignKeyNotification, object: window))
    try await settle("the background window kept its announcement") { viewer.session.announcement == nil }
    window.windowDidBecomeKey(Notification(name: NSWindow.didBecomeKeyNotification, object: window))
    try await clock.untilSleeping(for: ViewerController.announceAfter, "regaining the keyboard never subscribed")
    clock.advance(by: ViewerController.announceAfter)
    try await settle("the focused window never announced again") { viewer.session.announcement != nil }
    viewer.setVisible(false)
    try await settle("the hidden window kept its announcement") { viewer.session.announcement == nil }
  }

  @Test("unchanged asking text keeps its original hold rather than waiting another second")
  func unchangedTextDoesNotRestartTheHold() async throws {
    let clock = ManualClock()
    let viewer = ViewerController.scripted(ScriptedService(), clock: clock)
    viewer.announceSoon("Stopped")
    try await clock.untilSleeping(for: ViewerController.announceAfter, "the announcement never waited")
    clock.advance(by: .milliseconds(500))
    viewer.announceSoon("Stopped")
    clock.advance(by: .milliseconds(500))
    try await settle("unchanged text restarted the hold") { viewer.session.announcement == "Stopped" }
    viewer.close()
  }

  @Test("VoiceOver hears only a state that asks something of the person, once it has held for a second")
  func onlyAskingStatesAreAnnounced() async throws {
    let service = ScriptedService()
    service.reply = ["ready": true, "canObserve": true, "overview": ["mode": "agent", "state": "working"]]
    let clock = ManualClock()
    let viewer = ViewerController.scripted(service, clock: clock)
    viewer.session.paused = true
    viewer.setVisible(true)
    await viewer.readingLoop.readStatus()
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
