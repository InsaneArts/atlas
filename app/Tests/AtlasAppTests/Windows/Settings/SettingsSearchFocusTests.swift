import AppKit
import AsyncSupportTesting
import AtlasCommandsTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings search focus", .serialized, AppKitState())
@MainActor
struct SettingsSearchFocusTests {
  @Test(
    "in the Settings window a press outside the search takes the typing from it, and one on the field keeps it",
    .enabled(
      if: ProcessInfo.processInfo.environment["ATLAS_NATIVE_WINDOW_TESTS"] == "1",
      "requires native event-to-window routing in the hidden helper graphical test host"
    )
  )
  func aPressOutsideTheSearchTakesItsFocus() async throws {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]))
    let window = SettingsWindow(controller: controller) { _ in }
    defer { window.close() }
    window.present()
    let content = try #require(window.contentView)
    try await Self.settle(content) { Self.field(in: content) != nil }
    let field = try #require(Self.field(in: content), "the sidebar hosts its search field")
    window.makeFirstResponder(field)
    try await Self.settle(content) {
      window.firstResponder is NSTextView && window.fieldFocus.fieldFrame.map { !$0.isEmpty } == true
    }
    let frame = try #require(window.fieldFocus.fieldFrame, "the search says where it stands")
    window.fieldFocus.pressed(
      try SidebarSearchFocusTests.press(at: CGPoint(x: frame.midX, y: frame.midY), in: window)
    )
    #expect(window.firstResponder is NSTextView, "a press on the field keeps its typing")
    let outside = try SidebarSearchFocusTests.press(at: CGPoint(x: frame.maxX + 200, y: frame.midY + 200), in: window)
    try #require(outside.window === window, "the press must belong to the Settings window")
    window.fieldFocus.pressed(outside)
    try await Self.settle(content) { !(window.firstResponder is NSTextView) }
    #expect(!(window.firstResponder is NSTextView), "a press on the card takes the typing from the search")
  }

  /// The first text field in a view's tree: the sidebar's search.
  ///
  /// - Parameter view: The view.
  /// - Returns: The field, or nil.
  private static func field(in view: NSView) -> NSTextField? {
    if let field = view as? NSTextField, field.isEditable { return field }
    return view.subviews
      .lazy
      .compactMap { field(in: $0) }
      .first
  }

  /// Lays out and draws until `done`, failing past a bound only a stalled test reaches.
  ///
  /// - Parameters:
  ///   - view: The window's content.
  ///   - done: Whether the change arrived.
  /// - Throws: When it never does.
  private static func settle(_ view: NSView, until done: () -> Bool) async throws {
    let deadline = ContinuousClock.now
      .advanced(by: Ceiling.work)
    while !done(), ContinuousClock.now < deadline {
      view.layoutSubtreeIfNeeded()
      view.display()
      drainRunLoop()
      await afterQueuedWork()
    }
    try #require(done(), "the expected focus change never arrived")
  }

  /// Runs what the main run loop has waiting, once.
  private static func drainRunLoop() {
    CFRunLoopRunInMode(.defaultMode, .zero, true)
  }
}
