import AppKit
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
    for _ in 0..<20 { await Task.yield() }
    try await press("f", code: 3, modifiers: .command)
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
