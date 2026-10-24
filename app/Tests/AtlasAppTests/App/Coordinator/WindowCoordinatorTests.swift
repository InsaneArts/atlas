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

  @Test("⌘Q closes every window and the panel while Atlas keeps running", arguments: 0..<4)
  func commandQClosesEveryWindow(keyWindow: Int) async throws {
    let parts = StandInParts()
    let panel = PanelController.scripted(ScriptedService())
    parts.panel = panel
    let coordinator = parts.coordinator()
    coordinator.showAlert(.recover)
    coordinator.openViewer()
    coordinator.openSettings()
    coordinator.showOnboarding()
    try #require(parts.windows.count == 4)
    parts.windows[keyWindow].isKeyWindow = true
    coordinator.closeAllWindows()
    try await settle("Atlas never closed its panel") { parts.panelClosures == 1 }
    #expect(parts.windows.allSatisfy { !$0.isPresented })
    #expect(coordinator.viewer == nil)
    #expect(coordinator.settings == nil)
    #expect(coordinator.onboarding == nil)
    #expect(coordinator.alert == nil)
    #expect(parts.dockIcon.last == false)
    #expect(parts.panelClosures == 1)
    #expect(parts.panel === panel)
    #expect(parts.terminations == 0)
    coordinator.openViewer()
    #expect(coordinator.viewer?.window.isPresented == true)
  }

  @Test("⌘Q waits for the release answer before closing any window", arguments: [true, false])
  func commandQReleasesBeforeClosing(fromPanel: Bool) async throws {
    let service = ScriptedService()
    let controller = try await ViewerController.watching(service)
    let parts = StandInParts()
    parts.makeViewer = { controller }
    let coordinator = parts.coordinator()
    coordinator.showAlert(.recover)
    coordinator.openViewer()
    coordinator.openSettings()
    coordinator.showOnboarding()
    await controller.control.takeControl()
    try #require(controller.control.lease == "lease")
    service.hold = ["release"]
    #expect(coordinator.holdsUpQuitting)
    if fromPanel {
      coordinator.panelLinks()
        .closeWindows()
    } else {
      coordinator.closeAllWindows()
    }
    try await settle("the release never reached the service") { service.held["release"] != nil }
    let clock = try #require(parts.clock as? ManualClock)
    coordinator.closeAllWindows()
    clock.advance(by: WindowCoordinator.quitWait)
    #expect(parts.windows.allSatisfy { $0.isPresented })
    #expect(parts.terminations == 0)
    service.resume("release")
    try await settle("Atlas never closed its panel") { parts.panelClosures == 1 }
    #expect(parts.windows.allSatisfy { !$0.isPresented })
    #expect(!controller.barrier.isBusy)
    #expect(controller.control.lease == nil)
    #expect(parts.terminations == 0)
    controller.close()
  }

  @Test("the panel's Quit Atlas releases control before terminating")
  func panelQuitReleasesBeforeTerminating() async throws {
    let service = ScriptedService()
    let controller = try await ViewerController.watching(service)
    let parts = StandInParts()
    parts.makeViewer = { controller }
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    await controller.control.takeControl()
    service.hold = ["release"]
    var releasedFirst = false
    parts.quitting = {
      releasedFirst = service.log.contains("release lease") && !controller.barrier.isBusy
    }
    coordinator.panelLinks()
      .quit()
    try await settle("the release never reached the service") { service.held["release"] != nil }
    #expect(parts.terminations == 0)
    service.resume("release")
    try await settle("Atlas never quit") { parts.terminations == 1 }
    #expect(releasedFirst, "the release reached the service and was answered before Atlas quit")
    #expect(controller.control.lease == nil)
    #expect(!coordinator.holdsUpQuitting)
    controller.close()
  }

  @Test("AppKit termination from Dock quit, log-out or shutdown proceeds immediately without control")
  func terminationWithoutControl() {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    let reply = coordinator.prepareToTerminate { Issue.record("unexpectedly deferred termination") }
    #expect(reply == .terminateNow)
  }

  @Test("log-out or Dock quit proceeds after the answer or ceiling even while ⌘Q waits", arguments: [true, false])
  func terminationWithControl(releaseAnswers: Bool) async throws {
    let service = ScriptedService()
    let controller = try await ViewerController.watching(service)
    let parts = StandInParts()
    let clock = try #require(parts.clock as? ManualClock)
    parts.makeViewer = { controller }
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    await controller.control.takeControl()
    service.hold = ["release"]
    let replies = CallCount()
    coordinator.closeAllWindows()
    let reply = coordinator.prepareToTerminate { replies.count += 1 }
    #expect(reply == .terminateLater)
    try await settle("termination never sent the release") { service.held["release"] != nil }
    try await clock.untilSleeping(for: WindowCoordinator.quitWait, "termination never started its ceiling")
    #expect(replies.count == 0)
    if releaseAnswers {
      service.resume("release")
    } else {
      clock.advance(by: WindowCoordinator.quitWait)
    }
    try await settle("AppKit was never allowed to terminate") { replies.count == 1 }
    #expect(controller.barrier.isBusy == !releaseAnswers)
    service.resume("release")
    try await settle("the release never settled") { !controller.barrier.isBusy }
    controller.close()
  }

  @Test(
    "Copy Diagnostics reports success and failure from Settings or setup without a viewer",
    arguments: [true, false]
  )
  func diagnosticsWithoutAViewer(settings: Bool) async {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    if settings { coordinator.openSettings() } else { coordinator.showOnboarding() }
    await coordinator.copyDiagnostics()
    parts.diagnostics = false
    await coordinator.copyDiagnostics()
    #expect(parts.diagnosticNotices == [MenuWords.diagnosticsCopied, MenuWords.diagnosticsFailed])
    #expect(coordinator.viewer == nil)
  }

  @Test("Copy Diagnostics says in the main window whether it copied")
  func copyDiagnosticsSaysWhetherItCopied() async throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.openViewer()
    parts.windows.first?.isKeyWindow = true
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
