import AppKit
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("Sidebar search focus", .serialized, AppKitState())
@MainActor
struct SidebarSearchFocusTests {
  @Test("opening the search beside the card puts the typing in its field")
  func openingTheSearchFocusesItsField() throws {
    let closed = PreviewStates.viewerSessions
    let open = PreviewStates.viewer(closed, applying: .openSearch)
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: PreviewStates.viewerSize),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    let hosting = NSHostingView(rootView: ViewerHostContent(state: closed))
    window.contentView = hosting
    hosting.layoutSubtreeIfNeeded()
    // The field comes in the same change that asks for the focus.
    hosting.rootView = ViewerHostContent(state: open)
    hosting.layoutSubtreeIfNeeded()
    RunLoop.main
      .run(until: Date(timeIntervalSinceNow: Self.settling))
    #expect(window.firstResponder is NSTextView, "the field editor of the search has the focus")
  }

  @Test("a press anywhere outside the search takes the typing from it, and a press on its own capsule keeps it")
  func aPressOutsideTheSearchTakesItsFocus() throws {
    let closed = PreviewStates.viewerSessions
    let open = PreviewStates.viewer(closed, applying: .openSearch)
    let focus = FieldFocus()
    var sent: [ViewerEvent] = []
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: PreviewStates.viewerSize),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    let content = { (state: ViewerState) in
      ViewerView(state: state, send: { sent.append($0) }, input: { EmptyView() }) { _ in Color.clear }
        .environment(\.focusedFieldReport, focus.report)
        .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
    }
    let hosting = NSHostingView(rootView: content(closed))
    window.contentView = hosting
    hosting.layoutSubtreeIfNeeded()
    hosting.rootView = content(open)
    hosting.layoutSubtreeIfNeeded()
    RunLoop.main
      .run(until: Date(timeIntervalSinceNow: Self.settling))
    #expect(window.firstResponder is NSTextView, "the open search holds the typing")
    let field = try #require(focus.fieldFrame, "the focused field says where it stands")
    focus.watch(window)
    // On the capsule's margin, past its magnifier: still the field's own.
    focus.pressed(try Self.press(at: CGPoint(x: field.minX + Self.margin, y: field.midY), in: window))
    #expect(window.firstResponder is NSTextView, "a press on the field keeps its typing")
    // On the card, far from the sidebar.
    focus.pressed(try Self.press(at: CGPoint(x: field.maxX + Self.away, y: field.midY + Self.away), in: window))
    RunLoop.main
      .run(until: Date(timeIntervalSinceNow: Self.settling))
    #expect(!(window.firstResponder is NSTextView), "a press on the card takes the typing from the field")
    #expect(sent.contains(.closeSearch), "the empty search folds back to its circle")
    focus.stop()
  }

  /// A left press at `point`, in the window content's space from its top left.
  ///
  /// - Parameters:
  ///   - point: Where it lands.
  ///   - window: The window.
  /// - Returns: The press.
  /// - Throws: When AppKit makes no event.
  static func press(at point: CGPoint, in window: NSWindow) throws -> NSEvent {
    let height = window.contentView?.bounds.height ?? .zero
    return try #require(
      NSEvent.mouseEvent(
        with: .leftMouseDown,
        location: NSPoint(x: point.x, y: height - point.y),
        modifierFlags: [],
        timestamp: .zero,
        windowNumber: window.windowNumber,
        context: nil,
        eventNumber: .zero,
        clickCount: 1,
        pressure: 1
      )
    )
  }

  /// 4: how far into the capsule a press on its margin lands, short of the magnifier.
  static let margin: CGFloat = 4
  /// 200: how far past the field a press on the card lands.
  static let away: CGFloat = 200

  /// 0.2 s: long enough for the field to appear and take the focus.
  static let settling: TimeInterval = 0.2
}
