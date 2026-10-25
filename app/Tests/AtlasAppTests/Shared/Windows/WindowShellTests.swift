import AppKit
import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("WindowShell", .serialized, AppKitState())
struct WindowShellTests {
  @Test("the setup shell finishes closing only after the helper's marker is cleared")
  func setupCloseWaitsForItsMarker() async throws {
    let gate = ReleaseSignal()
    defer { gate.open() }
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false),
      "onboarding window": #"{"ok": true}"#,
    ])
    let closed = Action.markWindow(open: false).arguments
    commands.whileRunning { arguments in
      if arguments == closed { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands)
    let callbacks = CallCount()
    let window = OnboardingWindow(controller: controller) { _ in callbacks.count += 1 }
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      window.windowWillClose(Notification(name: NSWindow.willCloseNotification, object: window))
      try await settle("the shell never cleared its marker") { commands.ran.contains(closed) }
      #expect(callbacks.count == 0)
      gate.open()
      try await settle("the shell never completed its close") { callbacks.count == 1 }
    }
  }

  @Test("every window the coordinator opens is hosted by the shell in the dark appearance")
  func everyWindowIsHostedByTheShellInTheDark() throws {
    let service = ScriptedService()
    let commands = ScriptedCommands([:])
    let actions = HelperActions(
      commands: commands,
      service: service,
      barrier: ControlBarrier(service: service, clock: ManualClock())
    )
    let live = WindowMaking.live
    let windows: [any WindowPresenting] = [
      live.viewer(.scripted(service)) { _ in },
      live.settings(.scripted(commands: commands)) { _ in },
      live.onboarding(OnboardingController.scripted(commands)) { _ in },
      live.alert(AlertController(alert: .recover, actions: actions, links: AlertLinks(clock: ManualClock()))),
    ]
    for (index, made) in windows.enumerated() {
      let window = try #require(made as? NSWindow)
      #expect(window.appearance?.name == Appearance.dark?.name)
      #expect(!window.isReleasedWhenClosed)
      // The alert, last, is clear so its own corners and shadow show; every other window sits on the ground.
      #expect(window.backgroundColor == (index == windows.count - 1 ? NSColor.clear : NSColor.ground))
      let host = window.contentViewController.map { String(describing: type(of: $0)) }
      let view = window.contentView.map { String(describing: type(of: $0)) }
      #expect(
        host?.hasPrefix("NSHostingController<ShellRoot<") == true || view?.hasPrefix("NSHostingView<ShellRoot<") == true
      )
    }
  }
}
