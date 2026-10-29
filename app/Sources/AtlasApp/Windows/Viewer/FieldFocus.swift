import AppKit
import AtlasUI

/// Gives a window's text field up its keyboard focus when the person presses anywhere else in the window, as AppKit's
/// own fields do in a form: the sidebar, a row, the card, the screen. The press still does its own job, since it is
/// only looked at on its way; a press on any part of the field, its magnifier and its margins included, keeps the
/// focus, as the field reports where it stands while it holds the keyboard (``FocusedFieldReport``). A press on the
/// helper's screen while in control then hands the keyboard to the screen, which takes it on that press.
@MainActor
final class FieldFocus {
  /// The focused field's frame, in the window content's space from its top left; nil while no field reports one.
  private(set) var fieldFrame: CGRect?
  /// The window it watches.
  private weak var window: NSWindow?
  /// The press monitor, while it watches.
  private var monitor: Any?

  /// The report the window's content hands its fields.
  var report: FocusedFieldReport {
    FocusedFieldReport { [weak self] frame in self?.fieldFrame = frame }
  }

  /// Watches `window`'s presses, until ``stop()``.
  ///
  /// - Parameter window: The window.
  func watch(_ window: NSWindow) {
    self.window = window
    guard monitor == nil else { return }
    // AppKit runs a local monitor's handler on the main thread, and its handler type is not `@Sendable`, so the
    // handler shares this object's main-actor isolation and reads the event where it was delivered.
    monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown]) {
      [weak self] event in
      self?.pressed(event)
      return event
    }
  }

  /// Stops watching.
  func stop() {
    if let monitor { NSEvent.removeMonitor(monitor) }
    monitor = nil
  }

  /// A press in the window: outside the field holding the keyboard, the field gives it up, and the press goes on.
  ///
  /// - Parameter event: The press.
  func pressed(_ event: NSEvent) {
    guard let window, event.window === window,
      let editor = window.firstResponder as? NSTextView, editor.isFieldEditor,
      let content = window.contentView
    else { return }
    let frameView = content.superview ?? content
    let hitsField = Self.isOnField(frameView.hitTest(event.locationInWindow), editor: editor)
    let point = Self.point(event.locationInWindow, in: content)
    guard Self.releases(at: point, field: fieldFrame, hitsField: hitsField) else { return }
    window.makeFirstResponder(nil)
  }

  /// Whether the view a press landed on is the focused field's own: its editor, or the field the editor edits.
  ///
  /// - Parameters:
  ///   - hit: The view the press landed on.
  ///   - editor: The field editor holding the keyboard.
  /// - Returns: Whether it is the field's.
  private static func isOnField(_ hit: NSView?, editor: NSTextView) -> Bool {
    guard let hit else { return false }
    if hit.isDescendant(of: editor) { return true }
    guard let field = editor.delegate as? NSView else { return false }
    return hit.isDescendant(of: field)
  }

  /// Whether a press gives the field up its focus: unless it lands on the field's own view, or inside the frame the
  /// field reported, which takes in its magnifier and margins.
  ///
  /// - Parameters:
  ///   - point: Where the press landed, in the window content's space from its top left.
  ///   - field: The focused field's frame, if it reported one.
  ///   - hitsField: Whether the press lands on the field's own view.
  /// - Returns: Whether the field gives up its focus.
  nonisolated static func releases(at point: CGPoint, field: CGRect?, hitsField: Bool) -> Bool {
    if hitsField { return false }
    guard let field else { return true }
    return !field.contains(point)
  }

  /// A point in the window, in `content`'s space from its top left, as SwiftUI's global space runs.
  ///
  /// - Parameters:
  ///   - point: The point, in the window's space.
  ///   - content: The window's content view.
  /// - Returns: The point from the content's top left.
  private static func point(_ point: NSPoint, in content: NSView) -> CGPoint {
    let local = content.convert(point, from: nil)
    return content.isFlipped ? local : CGPoint(x: local.x, y: content.bounds.height - local.y)
  }
}
