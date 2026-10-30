import AppKit
import AsyncSupportTesting
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasUI

@Suite(
  "Activity fold focus",
  .serialized,
  AppKitState(),
  .enabled(
    if: ProcessInfo.processInfo.environment["ATLAS_NATIVE_WINDOW_TESTS"] == "1",
    "requires the hidden helper graphical test host"
  )
)
@MainActor
struct ActivityFoldFocusTests {
  @Test("folding a moment moves focus from its retained Show more button to the header and skips the invisible action")
  func foldedStepsCannotKeepKeyboardFocus() async throws {
    var moment = ViewerActivityMoment(
      id: "moment",
      icon: .atlas,
      title: "Commands",
      time: "10:21",
      span: "9 actions",
      age: "now",
      isOpen: true
    )
    moment.more = ViewerMoreSteps(moment: moment.id, count: 1)
    var sent: [ViewerEvent] = []
    let hosting = NSHostingView(rootView: ActivityMomentBlock(moment: moment, send: { sent.append($0) }))
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
      styleMask: .titled,
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    window.contentView = hosting
    defer { window.close() }
    window.makeKeyAndOrderFront(nil)
    try await Self.render(hosting) { Self.focusedLabel() != nil }
    for _ in 0..<10 {
      if Self.focusedLabel() == "Show 1 more" { break }
      window.selectNextKeyView(nil)
      hosting._renderForTest(interval: .zero)
      await afterQueuedWork()
    }
    try #require(Self.focusedLabel() == "Show 1 more")
    moment.isOpen = false
    moment.more = nil
    hosting.rootView = ActivityMomentBlock(moment: moment, send: { sent.append($0) })
    try await Self.render(hosting) { Self.focusedLabel() == moment.label }
    for _ in 0..<4 {
      window.selectNextKeyView(nil)
      hosting._renderForTest(interval: .zero)
      await afterQueuedWork()
      #expect(Self.focusedLabel() != "Show 1 more", "retained rows cannot enter the keyboard loop")
    }
    #expect(sent.isEmpty)
  }

  private static func focusedLabel() -> String? {
    let element = NSApplication.accessibilityFocusedUIElement() as? any NSAccessibilityProtocol
    return element?.accessibilityLabel()
  }

  private static func render<Content: View>(_ hosting: NSHostingView<Content>, until done: () -> Bool) async throws {
    let deadline = ContinuousClock.now
      .advanced(by: Ceiling.work)
    while !done(), ContinuousClock.now < deadline {
      hosting.layoutSubtreeIfNeeded()
      hosting._renderForTest(interval: .zero)
      await afterQueuedWork()
    }
    try #require(done(), "the expected native focus destination never arrived")
  }
}
