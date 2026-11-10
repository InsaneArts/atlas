import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Sidebar helper line click", .serialized, AppKitState())
@MainActor
struct SidebarHelperLineClickTests {
  @Test(
    "a click on the helper line, whole or folded into its circle, opens Settings on Helper",
    .enabled(
      if: ProcessInfo.processInfo.environment["ATLAS_NATIVE_WINDOW_TESTS"] == "1",
      "requires native event-to-window routing in the hidden helper graphical test host"
    ),
    arguments: [false, true]
  )
  func aClickOpensSettings(isCompact: Bool) async throws {
    var sent: [ViewerEvent] = []
    let helper = PreviewStates.viewerHelperLines[0].value
    let size = NSSize(width: ViewerLayout.sidebarWidth, height: ViewerLayout.helperCircle)
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: size),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    let hosting = NSHostingView(
      rootView: SidebarHelperLine(helper: helper, isCompact: isCompact) { sent.append($0) }
        .frame(width: size.width, height: size.height, alignment: .leading)
    )
    window.contentView = hosting
    defer { window.close() }
    window.makeKeyAndOrderFront(nil)
    hosting.layoutSubtreeIfNeeded()
    // On the mark, which both the row and the circle draw at the row's leading edge.
    let point = CGPoint(x: ViewerLayout.helperCircleInset + ViewerLayout.helperCircle / 2, y: size.height / 2)
    for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
      window.sendEvent(try SidebarSearchFocusTests.press(at: point, in: window, type: type))
      await afterQueuedWork()
    }
    #expect(sent == [SidebarHelperLine.click])
  }
}
