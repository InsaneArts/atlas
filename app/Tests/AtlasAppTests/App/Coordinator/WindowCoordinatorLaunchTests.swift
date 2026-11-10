import AppKit
import AsyncSupportTesting
import AtlasServiceTesting
import Testing

@testable import AtlasApp

@Suite("WindowCoordinator at launch", .serialized, AppKitState())
struct WindowCoordinatorLaunchTests {
  /// A coordinator over `parts` whose main window is the real one, so it closes as AppKit closes it, its controller
  /// keeping the same saved choices, as the app's does.
  private static func coordinator(_ parts: StandInParts, preferences: ViewerPreferences) -> WindowCoordinator {
    _ = NSApplication.shared
    parts.makeViewer = { .scripted(ScriptedService(), preferences: preferences) }
    var making = parts.making
    making.viewer = WindowMaking.live.viewer
    let coordinator = WindowCoordinator(windows: making, viewerPreferences: preferences) { _ in }
    coordinator.parts = parts
    return coordinator
  }

  @Test("a main window open when the app stopped opens again at the next launch")
  func anOpenWindowOpensAgain() throws {
    let preferences = ViewerPreferences.memory()
    let parts = StandInParts()
    let coordinator = parts.coordinator(preferences: preferences)
    coordinator.openViewer()
    #expect(preferences.loadWindowOpen())
    let next = StandInParts()
    let relaunched = next.coordinator(preferences: preferences)
    relaunched.launched()
    #expect(next.windows.count == 1)
    #expect(relaunched.viewer?.window.isPresented == true)
  }

  @Test("a main window the person closed stays closed at the next launch, though Atlas stops before the close settles")
  func aClosedWindowStaysClosed() throws {
    let preferences = ViewerPreferences.memory()
    let parts = StandInParts()
    let coordinator = Self.coordinator(parts, preferences: preferences)
    coordinator.openViewer()
    let window = try #require(coordinator.viewer?.window as? ViewerWindow)
    window.close()
    #expect(!preferences.loadWindowOpen())
    let next = StandInParts()
    next.coordinator(preferences: preferences)
      .launched()
    #expect(next.windows.isEmpty)
  }

  @Test("⌘Q forgets the main window, so the next launch opens none")
  func commandQForgetsTheWindow() async throws {
    let preferences = ViewerPreferences.memory()
    let parts = StandInParts()
    let coordinator = Self.coordinator(parts, preferences: preferences)
    coordinator.openViewer()
    coordinator.closeAllWindows()
    try await settle("Close All never finished") { parts.panelClosures == 1 }
    #expect(!preferences.loadWindowOpen())
  }

  @Test("a main window still open as Atlas quits stays remembered as open", arguments: ["quit", "terminate"])
  func quittingKeepsTheWindowRemembered(action: String) async throws {
    let preferences = ViewerPreferences.memory()
    let parts = StandInParts()
    let coordinator = parts.coordinator(preferences: preferences)
    coordinator.openViewer()
    if action == "quit" {
      coordinator.quit(nil)
      try await settle("Quit never finished") { parts.terminations == 1 }
    } else {
      #expect(coordinator.prepareToTerminate { Issue.record("no control needs a wait") } == .terminateNow)
    }
    #expect(preferences.loadWindowOpen())
  }

  @Test("a viewer line that arrives before launch finishes opens the main window once launch finishes")
  func anEarlyAskOpensOnceLaunched() throws {
    let parts = StandInParts()
    let coordinator = parts.coordinator()
    // Before launch the app has no parts to make a controller with.
    coordinator.parts = nil
    coordinator.openViewer(place: .activity)
    coordinator.openViewer()
    #expect(parts.windows.isEmpty)
    coordinator.parts = parts
    coordinator.launched()
    coordinator.launched()
    #expect(parts.windows.count == 1)
    #expect(parts.viewersMade == 1)
    #expect(coordinator.viewer?.controller.session.place == .activity)
  }

}
