import AppKit
import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("WindowShell")
struct WindowShellTests {
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
