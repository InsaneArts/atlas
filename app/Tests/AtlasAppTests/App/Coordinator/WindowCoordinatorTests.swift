import AppKit
import AsyncSupportTesting
import AtlasServiceTesting
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

  @Test("⌘Q quits Atlas whichever of its windows is in front")
  func quitQuitsFromAnyWindow() async throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    coordinator.showOnboarding()
    let viewer = try #require(parts.windows.first)
    viewer.isKeyWindow = true
    coordinator.quit(nil)
    try await settle("Atlas never quit") { parts.terminations == 1 }
  }

  @Test("⌘Q gives the main window's control back, and Atlas quits only once the service has answered the release")
  func quitReleasesControlBeforeQuitting() async throws {
    let service = ScriptedService()
    let controller = try await ViewerController.watching(service)
    let parts = StandInParts()
    parts.makeViewer = { controller }
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    await controller.control.takeControl()
    try #require(controller.control.lease == "lease")
    #expect(coordinator.holdsUpQuitting)
    var releasedFirst = false
    parts.quitting = {
      releasedFirst = service.log.contains("release lease") && !controller.barrier.isBusy
    }
    coordinator.quit(nil)
    try await settle("Atlas never quit") { parts.terminations == 1 }
    #expect(releasedFirst, "the release reached the service and was answered before Atlas quit")
    #expect(controller.control.lease == nil)
    #expect(!coordinator.holdsUpQuitting)
    controller.close()
  }

  @Test("Copy Diagnostics says in the main window whether it copied")
  func copyDiagnosticsSaysWhetherItCopied() async throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    await coordinator.copyDiagnostics()
    #expect(coordinator.viewer?.controller.session.notice == MenuWords.diagnosticsCopied)
    parts.diagnostics = false
    await coordinator.copyDiagnostics()
    #expect(coordinator.viewer?.controller.session.notice == MenuWords.diagnosticsFailed)
  }

  @Test("quitting waits at most two seconds for an unanswered release")
  func quittingHasABoundedReleaseWait() async throws {
    let service = ScriptedService()
    let controller = try await ViewerController.watching(service)
    let parts = StandInParts()
    let clock = try #require(parts.clock as? ManualClock)
    parts.makeViewer = { controller }
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    await controller.control.takeControl()
    try #require(controller.control.lease != nil)
    service.hold = ["release"]
    coordinator.quit(nil)
    try await settle("the release never reached the service") { service.held["release"] != nil }
    try await clock.untilSleeping(for: WindowCoordinator.quitWait, "quitting never started its ceiling")
    clock.advance(by: WindowCoordinator.quitWait - .milliseconds(1))
    #expect(parts.terminations == 0)
    clock.advance(by: .milliseconds(1))
    try await settle("Atlas never quit after its release ceiling") { parts.terminations == 1 }
    #expect(controller.barrier.isBusy, "the unanswered release still excludes another controller")
    service.resume("release")
    try await settle("the release never settled") { !controller.barrier.isBusy }
    controller.close()
  }

  @Test("Settings follows the panel when the helper reconnects after a failed reading")
  func settingsFollowsReconnecting() async {
    let service = ScriptedService()
    let parts = StandInParts()
    parts.panel = .scripted(service)
    let coordinator = parts.coordinator()
    let links = coordinator.settingsLinks()
    #expect(links.status().connecting)
    await parts.panel?.readStatus()
    #expect(!links.status().connecting)
    service.fail = ["status"]
    await parts.panel?.readStatus()
    #expect(links.status().connecting)
  }

  @Test("the panel's Open Settings shows Helper even when another Settings tab was selected")
  func thePanelOpensHelperSettings() {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openSettings()
    coordinator.settings?.controller.selectTab(.general)
    let links = coordinator.panelLinks()
    links.openSettings()
    #expect(coordinator.settings?.controller.state.tabs.selected == .helper)
  }

  @Test("the Dock's menu offers the main window's control, agent and picture while it is open, and nothing else")
  func theDockMenuFollowsTheMainWindow() throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    #expect(coordinator.dockMenu() == nil, "no main window, no menu")
    coordinator.openViewer()
    let menu = try #require(coordinator.dockMenu())
    #expect(
      menu.items.map(\.action) == [
        #selector(ViewerMenuActions.toggleControl(_:)), #selector(ViewerMenuActions.toggleAgent(_:)),
        #selector(ViewerMenuActions.togglePicturePause(_:)),
      ]
    )
    #expect(menu.items.allSatisfy { $0.target === parts.windows.first })
  }
}
