import AppKit
import AsyncSupportTesting
import AtlasServiceTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite(
  "Viewer sidebar key dispatch",
  .serialized,
  AppKitState(),
  .enabled(
    if: ProcessInfo.processInfo.environment["ATLAS_NATIVE_WINDOW_TESTS"] == "1",
    "requires the hidden helper graphical test host"
  )
)
@MainActor
struct ViewerSidebarKeyDispatchTests {
  @Test(
    "native window dispatch preserves arrows and Space for a focused sidebar session row while Screenshots shows",
    arguments: [false, true]
  )
  func sidebarRowsOwnTheirKeys(overlay: Bool) async throws {
    _ = NSApplication.shared
    let viewer = ViewerController.scripted(ScriptedService())
    viewer.session.place = .screenshots
    viewer.session.overlayShown = overlay
    let window = ViewerWindow(controller: viewer) { _ in }
    defer { window.close() }
    var sent: [ViewerEvent] = []
    let state = PreviewStates.viewerSessions.sidebar.sessions
    let hosting = NSHostingView(
      rootView: SidebarSessionList(state: state) { sent.append($0) }
        .environment(\.initialSessionFocus, .all)
        .frame(width: ViewerLayout.sidebarWidth, height: PreviewStates.viewerSize.height)
    )
    window.contentView = hosting
    // This suite is opt-in only in the hidden helper; real native focus requires a key window.
    window.makeKeyAndOrderFront(nil)
    hosting.layoutSubtreeIfNeeded()
    for _ in 0..<6 {
      await afterQueuedWork()
      hosting._renderForTest(interval: .zero)
    }
    #expect(window.firstResponder !== window, "a real sidebar row must own native focus")
    window.sendEvent(try Self.key(ShotKeys.down, characters: "\u{f701}", in: window))
    for _ in 0..<6 {
      await afterQueuedWork()
      hosting._renderForTest(interval: .zero)
    }
    let choice = try #require(sent.last)
    #expect(choice == .chooseSession(state.choice(1, from: .all)))
    sent.removeAll()
    window.sendEvent(try Self.key(ShotKeys.space, characters: " ", in: window))
    #expect(sent.last == choice, "Space activates the focused session row rather than a screenshot")
  }

  private static func key(_ code: UInt16, characters: String, in window: NSWindow) throws -> NSEvent {
    try #require(
      NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: [],
        timestamp: .zero,
        windowNumber: window.windowNumber,
        context: nil,
        characters: characters,
        charactersIgnoringModifiers: characters,
        isARepeat: false,
        keyCode: code
      )
    )
  }
}
