import AppKit
import AsyncSupportTesting
import Testing

@testable import AtlasApp

/// Drives the Settings field through its native responder chain without showing a window.
final class SettingsSearchKeyboard {
  /// The real Settings shell and its controller-backed SwiftUI view.
  let window: SettingsWindow
  /// The shipped menu, with its responder destination fixed to this hidden test window.
  let menu = MainMenu.make(viewer: true)

  /// Hosts Settings with an isolated frame identity.
  ///
  /// - Parameter controller: Receives the view's actual key handling.
  init(_ controller: SettingsController) {
    window = SettingsWindow(controller: controller, autosaveName: "Settings-keyboard-\(UUID())", closed: { _ in })
    for item in menu.items.flatMap({ $0.submenu?.items ?? [] })
    where item.action == #selector(FindMenuActions.findInTab(_:)) {
      item.target = window
    }
    window.contentView?.layoutSubtreeIfNeeded()
  }

  /// Focuses the native search field with the shipped Command-F shortcut.
  ///
  /// - Returns: The field editor receiving the keys.
  /// - Throws: When the search field cannot take focus.
  func focus() async throws -> NSTextView {
    window.contentView?.layoutSubtreeIfNeeded()
    try await press("f", code: 3, modifiers: .command)
    try await waitForFocus { window.firstResponder is NSTextView }
    let editor = try #require(window.firstResponder as? NSTextView)
    let field = try #require(Self.fields(in: window.contentView).first)
    #expect(editor.delegate === field, "Find focuses the search field")
    return editor
  }

  /// Dispatches a native key event, then lets the view deliver its state changes.
  ///
  /// - Parameters:
  ///   - characters: The key's native characters.
  ///   - code: Its hardware key code.
  ///   - modifiers: Any editing modifiers held with the key.
  /// - Throws: When AppKit cannot construct the event.
  func press(_ characters: String, code: UInt16, modifiers: NSEvent.ModifierFlags = []) async throws {
    window.contentView?.layoutSubtreeIfNeeded()
    // Flush the next native focus update before sending a key to a newly restored SwiftUI control.
    await nativeUpdate()
    let event = try #require(
      NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: modifiers,
        timestamp: 0,
        windowNumber: window.windowNumber,
        context: nil,
        characters: characters,
        charactersIgnoringModifiers: characters,
        isARepeat: false,
        keyCode: code
      )
    )
    if !menu.performKeyEquivalent(with: event) { window.sendEvent(event) }
    for _ in 0..<20 { await Task.yield() }
    window.contentView?.layoutSubtreeIfNeeded()
  }

  /// Releases the test window and its isolated frame record.
  func close() {
    let name = window.frameAutosaveName
    window.close()
    NSWindow.removeFrame(usingName: name)
  }

  /// Waits for AppKit's actual responder change, rather than a scheduler-dependent number of task yields.
  ///
  /// - Parameters:
  ///   - sourceLocation: Where an undelivered focus request is reported.
  ///   - changed: The responder condition that must hold.
  /// - Throws: When AppKit never delivers the requested focus.
  func waitForFocus(sourceLocation: SourceLocation = #_sourceLocation, _ changed: () -> Bool) async throws {
    let clock = ContinuousClock()
    let deadline = clock.now
      .advanced(by: .seconds(3))
    while !changed(), clock.now < deadline {
      window.contentView?.layoutSubtreeIfNeeded()
      await nativeUpdate()
    }
    try #require(changed(), "the native responder never received the requested focus", sourceLocation: sourceLocation)
  }

  /// Lets AppKit and SwiftUI deliver a queued native focus update on the main dispatch queue.
  private func nativeUpdate() async {
    await withCheckedContinuation { continuation in
      DispatchQueue.main
        .async { continuation.resume() }
    }
  }

  /// Finds native text fields without depending on SwiftUI's private host classes.
  ///
  /// - Parameter view: A view in the Settings hierarchy.
  /// - Returns: Its text-field descendants in tree order.
  private static func fields(in view: NSView?) -> [NSTextField] {
    guard let view else { return [] }
    return view.subviews.flatMap { child in
      (child as? NSTextField)
        .map { [$0] } ?? fields(in: child)
    }
  }
}
