import AppKit
import AsyncSupportTesting
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("Sidebar search focus", .serialized, AppKitState())
@MainActor
struct SidebarSearchFocusTests {
  @Test("opening the search beside the card puts the typing in its field")
  func openingTheSearchFocusesItsField() async throws {
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
    defer { window.contentView = nil }
    hosting.layoutSubtreeIfNeeded()
    // The field comes in the same change that asks for the focus.
    hosting.rootView = ViewerHostContent(state: open)
    hosting.layoutSubtreeIfNeeded()
    try await Self.render(hosting, until: { window.firstResponder is NSTextView })
    #expect(window.firstResponder is NSTextView, "the field editor of the search has the focus")
  }

  @Test(
    "a press anywhere outside the search takes the typing from it, and a press on its own capsule keeps it",
    .enabled(
      if: ProcessInfo.processInfo.environment["ATLAS_NATIVE_WINDOW_TESTS"] == "1",
      "requires native event-to-window routing in the hidden helper graphical test host"
    )
  )
  func aPressOutsideTheSearchTakesItsFocus() async throws {
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
    defer {
      focus.stop()
      window.close()
    }
    window.makeKeyAndOrderFront(nil)
    hosting.layoutSubtreeIfNeeded()
    hosting.rootView = content(open)
    hosting.layoutSubtreeIfNeeded()
    try await Self.render(
      hosting,
      until: {
        window.firstResponder is NSTextView && focus.fieldFrame.map { !$0.isNull && !$0.isEmpty } == true
      }
    )
    #expect(window.firstResponder is NSTextView, "the open search holds the typing")
    let field = try #require(focus.fieldFrame, "the focused field says where it stands")
    focus.watch(window)
    // On the capsule's margin, past its magnifier: still the field's own.
    focus.pressed(try Self.press(at: CGPoint(x: field.minX + Self.margin, y: field.midY), in: window))
    #expect(window.firstResponder is NSTextView, "a press on the field keeps its typing")
    // On the card, far from the sidebar.
    let outside = try Self.press(at: CGPoint(x: field.maxX + Self.away, y: field.midY + Self.away), in: window)
    try #require(outside.window === window, "the press must belong to the hosted window")
    focus.pressed(outside)
    try await Self.render(hosting, until: { !(window.firstResponder is NSTextView) && sent.contains(.closeSearch) })
    #expect(!(window.firstResponder is NSTextView), "a press on the card takes the typing from the field")
    #expect(sent.contains(.closeSearch), "the empty search folds back to its circle")
    focus.stop()
  }

  @Test("dismissing a narrow sidebar overlay relinquishes its search focus without focusing a hidden field")
  func dismissingTheOverlayRelinquishesSearchFocus() async throws {
    var state = PreviewStates.viewerSessions
    state.sidebar.mode = .hidden
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: PreviewStates.viewerSize),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    let hosting = NSHostingView(rootView: ViewerHostContent(state: state))
    window.contentView = hosting
    defer { window.contentView = nil }
    hosting.layoutSubtreeIfNeeded()
    state.sidebar.overlay = true
    state.sidebar.search.isOpen = true
    state.sidebar.search.query = "Claude"
    hosting.rootView = ViewerHostContent(state: state)
    hosting.layoutSubtreeIfNeeded()
    try await Self.render(hosting, until: { window.firstResponder is NSTextView })
    try #require(window.firstResponder is NSTextView, "the visible overlay search holds the typing")
    state.sidebar.overlay = false
    hosting.rootView = ViewerHostContent(state: state)
    hosting.layoutSubtreeIfNeeded()
    try await Self.render(hosting, until: { !(window.firstResponder is NSTextView) })
    #expect(!(window.firstResponder is NSTextView), "a hidden search cannot keep the typing")
    #expect(state.sidebar.search.query == "Claude", "dismissing preserves the query without retaining an editor")
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

  /// Drives rendering until the expected focus change, using elapsed time only to fail a stalled test.
  private static func render<Content: View>(
    _ hosting: NSHostingView<Content>,
    sourceLocation: SourceLocation = #_sourceLocation,
    until done: () -> Bool
  ) async throws {
    let deadline = ContinuousClock.now
      .advanced(by: Ceiling.work)
    while !done(), ContinuousClock.now < deadline {
      hosting.layoutSubtreeIfNeeded()
      hosting._renderForTest(interval: .zero)
      drainRunLoop()
      await afterQueuedWork()
    }
    try #require(done(), "the expected focus and render change never arrived", sourceLocation: sourceLocation)
  }

  private static func drainRunLoop() {
    CFRunLoopRunInMode(.defaultMode, .zero, true)
  }
}
