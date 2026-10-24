import Testing

@testable import AtlasApp

@Suite("WindowCoordinator")
struct WindowCoordinatorTests {
  @Test("opening the viewer while it is open brings the one window forward")
  func openingTheViewerAgainBringsItForward() throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    coordinator.openViewer(tab: .activity)
    let window = try #require(parts.windows.first)
    #expect(parts.windows.count == 1)
    #expect(parts.viewersMade == 1)
    #expect(window.presentations == 2)
    #expect(coordinator.viewer?.controller.session.tab == .activity)
  }

  @Test("opening Settings builds its controller once")
  func settingsBuildsItsControllerOnce() throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openSettings()
    let first = try #require(coordinator.settings?.controller)
    try #require(parts.windows.first)
      .dismiss()
    coordinator.openSettings()
    #expect(parts.windows.count == 2)
    #expect(parts.settingsMade == 1)
    #expect(coordinator.settings?.controller === first)
  }

  @Test("the Dock icon shows while a window is open and hides when the last one closes")
  func theDockIconFollowsTheOpenWindows() throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    coordinator.openSettings()
    let viewer = try #require(parts.windows.first)
    let settings = try #require(parts.windows.last)
    viewer.dismiss()
    #expect(parts.dockIcon == [true, true, true])
    settings.dismiss()
    #expect(parts.dockIcon == [true, true, true, false])
    #expect(coordinator.viewer == nil)
    #expect(coordinator.settings == nil)
  }

  @Test("a viewer reopened while its old window is still closing stays open when the old one finishes")
  func aLateCloseLeavesTheReopenedViewer() throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    let old = try #require(parts.windows.first)
    old.closesLater = true
    old.dismiss()
    coordinator.openViewer()
    let new = try #require(parts.windows.last)
    old.finishClosing()
    #expect(parts.windows.count == 2)
    #expect(parts.viewersMade == 2)
    #expect(coordinator.viewer?.window === new)
    #expect(parts.dockIcon.last == true)
  }

  @Test("⌘Q with no window of Atlas's in front quits Atlas")
  func quitWithNoKeyWindowQuits() throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    coordinator.quit(nil)
    #expect(parts.terminations == 1)
    #expect(try #require(parts.windows.first).isPresented)
  }

  @Test("⌘Q closes the key window and leaves the others open")
  func quitClosesTheKeyWindow() throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    coordinator.showOnboarding()
    let viewer = try #require(parts.windows.first)
    let setup = try #require(parts.windows.last)
    viewer.isKeyWindow = true
    coordinator.quit(nil)
    #expect(!viewer.isPresented)
    #expect(setup.isPresented)
    #expect(parts.terminations == 0)
  }
}
