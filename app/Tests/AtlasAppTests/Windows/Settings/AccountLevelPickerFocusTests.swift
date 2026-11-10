import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Account level picker focus", .serialized, AppKitState())
@MainActor
struct AccountLevelPickerFocusTests {
  @Test(
    "the level picker keeps the keyboard's focus through a change, so the next arrow works without focusing it again",
    .enabled(
      if: ProcessInfo.processInfo.environment["ATLAS_NATIVE_WINDOW_TESTS"] == "1",
      "requires native event-to-window routing in the hidden helper graphical test host"
    )
  )
  func theFocusOutlivesABusySpell() async throws {
    var picked: [AccountLevelChoice] = []
    let size = NSSize(width: SettingsLayout.pageWidth, height: LevelTileLayout.height * 2)
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: size),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    let picker = { (level: AccountLevelChoice, isBusy: Bool) in
      AccountLevelPicker(account: "github.com", level: level, isBusy: isBusy) { picked.append($0) }
        .frame(width: size.width, height: size.height)
    }
    let hosting = NSHostingView(rootView: picker(.lookOnly, false))
    window.contentView = hosting
    defer { window.close() }
    window.makeKeyAndOrderFront(nil)
    hosting.layoutSubtreeIfNeeded()
    // Tab takes the keyboard's focus into the picker, as the person does.
    try await Self.press(.tab, in: window)
    try await Self.press(.rightArrow, in: window)
    #expect(picked == [.ask], "the focused picker steps on the right arrow")
    hosting.rootView = picker(.ask, true)
    await Self.settle(hosting)
    try await Self.press(.rightArrow, in: window)
    #expect(picked == [.ask], "no pick goes through while the change runs")
    hosting.rootView = picker(.ask, false)
    await Self.settle(hosting)
    try await Self.press(.rightArrow, in: window)
    #expect(picked == [.ask, .trusted], "the focus held, so the next arrow steps again")
  }

  /// A key sent to `window` as the person presses it: down, then up.
  ///
  /// - Parameters:
  ///   - key: The key.
  ///   - window: The window.
  /// - Throws: When AppKit makes no event.
  private static func press(_ key: KeyEquivalent, in window: NSWindow) async throws {
    let characters = String(key.character)
    let code: UInt16 =
      switch key {
      case .tab: 48
      case .rightArrow: 124
      default: 0
      }
    for type in [NSEvent.EventType.keyDown, .keyUp] {
      let event = try #require(
        NSEvent.keyEvent(
          with: type,
          location: .zero,
          modifierFlags: key == .rightArrow ? [.numericPad, .function] : [],
          timestamp: ProcessInfo.processInfo.systemUptime,
          windowNumber: window.windowNumber,
          context: nil,
          characters: characters,
          charactersIgnoringModifiers: characters,
          isARepeat: false,
          keyCode: code
        )
      )
      window.sendEvent(event)
    }
    if let content = window.contentView { await settle(content) }
  }

  /// Lays out and lets queued work run, so the picker hears the key and its new state.
  ///
  /// - Parameter view: The window's content.
  private static func settle(_ view: NSView) async {
    for _ in 0..<6 {
      view.layoutSubtreeIfNeeded()
      await afterQueuedWork()
    }
  }
}
