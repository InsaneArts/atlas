import AppKit
import AsyncSupportTesting
import AtlasUI
import SwiftUI
import Testing

@Suite("Viewer screenshot scrolling", .serialized, AppKitState())
struct ViewerShotsScrollTests {
  @Test("a keyboard move reveals a distant selected screenshot while an ordinary refresh keeps the scroll position")
  func keyboardSelectionScrollsIntoView() async throws {
    let shots = (0..<40)
      .map { index in
        ViewerShotTile(id: "shot-\(index)", time: "12:00", thumbnail: nil, isSelected: index == 0, label: "Screenshot")
      }
    var state = ViewerShotsState(meta: "40 today", days: [ViewerShotDay(id: "today", title: "Today", shots: shots)])
    let host = NSHostingView(rootView: ShotScrollHost(state: state))
    host.frame = NSRect(x: 0, y: 0, width: 560, height: 320)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    host.layoutSubtreeIfNeeded()
    let scrolls = Self.scrollViews(in: host)
    let scroll = try #require(scrolls.first)
    let initial = scroll.contentView.bounds.origin
    state.days[0].shots[0].isSelected = false
    state.days[0].shots[39].isSelected = true
    host.rootView = ShotScrollHost(state: state)
    host.layoutSubtreeIfNeeded()
    #expect(scroll.contentView.bounds.origin == initial, "refreshes do not reposition the grid")
    let scrolled = ReleaseSignal()
    scroll.contentView.postsBoundsChangedNotifications = true
    let observer = NotificationCenter.default
      .addObserver(
        forName: NSView.boundsDidChangeNotification,
        object: scroll.contentView,
        queue: .main
      ) { _ in
        Task { @MainActor in scrolled.open() }
      }
    defer {
      NotificationCenter.default
        .removeObserver(observer)
    }
    let moved = Task { await scrolled.wait() }
    defer { moved.cancel() }
    state.scrollRequest += 1
    host.rootView = ShotScrollHost(state: state)
    host.layoutSubtreeIfNeeded()
    try await moved.value("keyboard selection never scrolled")
    #expect(scroll.contentView.bounds.origin != initial, "keyboard selection reveals the last tile")
  }

  /// Native scroll views under a hosting view, in tree order.
  ///
  /// - Parameter view: The view to search.
  /// - Returns: Its scroll views.
  private static func scrollViews(in view: NSView) -> [NSScrollView] {
    let own =
      (view as? NSScrollView)
      .map { [$0] } ?? []
    return own + view.subviews.flatMap { scrollViews(in: $0) }
  }
}
