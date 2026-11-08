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
  @Test("folding a run moves focus from its retained Replay button to its row and skips the invisible steps")
  func foldedStepsCannotKeepKeyboardFocus() async throws {
    var run = ViewerOutlineRun(
      id: "run",
      icon: .atlas,
      name: "Commands",
      summary: "Ran a command · 1 action",
      time: "10:21",
      isOpen: true,
      canReplay: true,
      steps: [ViewerActivityStep(id: "step", kind: .command, words: ViewerStepWords("Ran a command"))]
    )
    var sent: [ViewerEvent] = []
    let hosting = NSHostingView(rootView: ActivityRunBlock(run: run, level: 0, send: { sent.append($0) }))
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
      if Self.focusedLabel() == "Replay" { break }
      window.selectNextKeyView(nil)
      hosting._renderForTest(interval: .zero)
      await afterQueuedWork()
    }
    try #require(Self.focusedLabel() == "Replay")
    run.isOpen = false
    run.steps = []
    hosting.rootView = ActivityRunBlock(run: run, level: 0, send: { sent.append($0) })
    try await Self.render(hosting) { Self.focusedLabel() == run.label }
    for _ in 0..<4 {
      window.selectNextKeyView(nil)
      hosting._renderForTest(interval: .zero)
      await afterQueuedWork()
      #expect(Self.focusedLabel() != "Replay", "retained rows cannot enter the keyboard loop")
    }
    #expect(sent.isEmpty)
  }

  @Test("Return on a focused Replay plays the run and leaves the run open")
  func returnOnReplayPlaysTheRun() async throws {
    let run = ViewerOutlineRun(
      id: "run",
      icon: .atlas,
      name: "Commands",
      summary: "Ran a command · 1 action",
      time: "10:21",
      isOpen: true,
      canReplay: true,
      steps: [ViewerActivityStep(id: "step", kind: .command, words: ViewerStepWords("Ran a command"))]
    )
    var sent: [ViewerEvent] = []
    let hosting = NSHostingView(rootView: ActivityRunBlock(run: run, level: 0, send: { sent.append($0) }))
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
      if Self.focusedLabel() == "Replay" { break }
      window.selectNextKeyView(nil)
      hosting._renderForTest(interval: .zero)
      await afterQueuedWork()
    }
    try #require(Self.focusedLabel() == "Replay")
    let press = try #require(
      NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: [],
        timestamp: .zero,
        windowNumber: window.windowNumber,
        context: nil,
        characters: "\r",
        charactersIgnoringModifiers: "\r",
        isARepeat: false,
        keyCode: Self.returnKey
      )
    )
    window.sendEvent(press)
    hosting._renderForTest(interval: .zero)
    await afterQueuedWork()
    #expect(sent == [.openReplay("run")], "Return plays the run and never folds it")
  }

  /// Return's key code.
  private static let returnKey: UInt16 = 36

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
