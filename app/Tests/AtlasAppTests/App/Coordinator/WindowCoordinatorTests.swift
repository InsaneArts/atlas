import AppKit
import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasService
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

  @Test("setup reopens once after its old close settles and leaves the helper marked open", arguments: [true, false])
  func setupReopensAfterItsOldClose(isHelper: Bool) async throws {
    let gate = ReleaseSignal()
    defer { gate.open() }
    let open = Action.markWindow(open: true).arguments
    let closed = Action.markWindow(open: false).arguments
    let commands = ScriptedCommands([
      "onboarding status": isHelper ? OnboardingReplies.helper(accessibility: false) : OnboardingReplies.freshMac,
      "onboarding window": #"{"ok": true}"#,
    ])
    commands.whileRunning { arguments in
      if arguments == closed { await gate.wait() }
    }
    let parts = StandInParts()
    parts.makeOnboarding = { .scripted(commands) }
    let coordinator = parts.coordinator()
    coordinator.showOnboarding()
    let old = try #require(coordinator.onboarding?.controller)
    let window = try #require(parts.windows.first)
    try await old.whileOpen { _ in
      try await old.untilLoaded()
      if isHelper { try await settle("setup never marked itself open") { commands.ran.contains(open) } }
      window.closesLater = true
      window.dismiss()
      coordinator.showOnboarding()
      coordinator.showOnboarding()
      #expect(parts.windows.count == 1, "the old close still owns the helper marker")
      let closing = Task {
        await old.close()
        window.finishClosing()
      }
      if isHelper {
        try await settle("the old close never cleared its marker") { commands.ran.contains(closed) }
        #expect(parts.windows.count == 1, "replacement waits for marker clearing to settle")
      }
      gate.open()
      await closing.value
      #expect(parts.windows.count == 2, "repeated opens make one replacement")
      let replacement = try #require(coordinator.onboarding?.controller)
      #expect(replacement !== old)
      try await replacement.whileOpen { _ in
        try await replacement.untilLoaded()
        if isHelper {
          try await settle("replacement never marked itself open") { commands.ran.count { $0 == open } == 2 }
          #expect(commands.ran.filter { $0.starts(with: ["onboarding", "window"]) } == [open, closed, open])
        } else {
          #expect(!commands.ran.contains { $0.starts(with: ["onboarding", "window"]) })
        }
      }
    }
  }

  @Test(
    "Close All and quit discard a setup reopening waiting for its old close",
    arguments: ["close", "quit", "terminate"]
  )
  func closingDiscardsSetupReopening(action: String) async throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    coordinator.showOnboarding()
    let window = try #require(parts.windows.first)
    window.closesLater = true
    window.dismiss()
    coordinator.showOnboarding()
    switch action {
    case "close": coordinator.closeAllWindows()
    case "quit": coordinator.quit(nil)
    default:
      #expect(coordinator.prepareToTerminate { Issue.record("no control needs a wait") } == .terminateNow)
    }
    coordinator.showOnboarding()
    window.finishClosing()
    if action == "close" {
      try await settle("Close All never finished") { parts.panelClosures == 1 }
    } else if action == "quit" {
      try await settle("Quit never finished") { parts.terminations == 1 }
    }
    #expect(parts.windows.count == 1)
    #expect(coordinator.onboarding == nil)
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

  @Test("AppKit termination retries a refused release even after the viewer is gone", arguments: [true, false])
  func terminationRetriesARefusedRelease(retrySucceeds: Bool) async throws {
    let service = ScriptedService()
    let parts = StandInParts()
    let barrier = ControlBarrier(service: service, clock: parts.clock)
    parts.controlBarrier = barrier
    service.releaseRefusal = ServiceError("Still held", code: "busy")
    #expect(await barrier.releaseNow("lease") != nil)
    #expect(!barrier.isBusy)
    #expect(barrier.unreleased == ["lease"])
    if retrySucceeds { service.releaseRefusal = nil }
    service.hold = ["release"]
    defer { service.resume("release") }
    let coordinator = parts.coordinator()
    let replies = CallCount()
    let reply = coordinator.prepareToTerminate { replies.count += 1 }
    try #require(reply == .terminateLater)
    try await settle("termination never retried the refused release") { service.held["release"] != nil }
    #expect(replies.count == 0)
    #expect(coordinator.viewer == nil)
    service.resume("release")
    try await settle("AppKit was never allowed to terminate") { replies.count == 1 }
    #expect(service.log.count { $0 == "release lease" } == 2)
    #expect(!barrier.isBusy)
    #expect(barrier.unreleased == (retrySucceeds ? [] : ["lease"]))
  }

  @Test("a refused release retried at termination is cancelled at the existing two-second ceiling")
  func terminationBoundsAStalledRetry() async throws {
    let secret = UUID().uuidString
    let requests = CallCount()
    let cancellations = CallCount()
    let session = RetriedReleaseProtocol.session(secret: secret, requests: requests, cancellations: cancellations)
    defer {
      session.invalidateAndCancel()
      RetriedReleaseProtocol.forget(secret)
    }
    let service = try #require(ViewerClient("http://127.0.0.1:8765/\(secret)/", session: session))
    let parts = StandInParts()
    let clock = try #require(parts.clock as? ManualClock)
    let barrier = ControlBarrier(service: service, clock: clock)
    parts.controlBarrier = barrier
    #expect(await barrier.releaseNow("lease") != nil)
    #expect(barrier.unreleased == ["lease"])
    #expect(!barrier.isBusy)
    let replies = CallCount()
    let coordinator = parts.coordinator()
    let reply = coordinator.prepareToTerminate { replies.count += 1 }
    try #require(reply == .terminateLater)
    try await settle("termination never sent the retry") { requests.count == 2 }
    try await clock.untilSleeping(for: WindowCoordinator.quitWait, "termination never started its ceiling")
    clock.advance(by: WindowCoordinator.quitWait - .milliseconds(1))
    #expect(replies.count == 0)
    #expect(cancellations.count == 0)
    clock.advance(by: .milliseconds(1))
    try await settle("the ceiling never cancelled the request and allowed termination") {
      replies.count == 1 && cancellations.count == 1
    }
    #expect(!barrier.isBusy)
    #expect(barrier.unreleased == ["lease"], "an unanswered retry is still pending")
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
