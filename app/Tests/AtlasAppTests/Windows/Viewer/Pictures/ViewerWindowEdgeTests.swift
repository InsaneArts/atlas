import AppKit
import AsyncSupportTesting
import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Viewer window edges", .serialized, AppKitState())
@MainActor
struct ViewerWindowEdgeTests {
  @Test("in the real window, Activity fills the card through the sidebar hiding and showing and the window zooming")
  func activityFillsTheCard() async throws {
    _ = NSApplication.shared
    let previous = Motion.Pose.moment
    Motion.Pose.hold(at: .zero)
    defer { Motion.Pose.hold(at: previous) }
    let service = ScriptedService()
    service.activityLog = Self.log
    let viewer = try await ViewerController.watching(service)
    defer { viewer.close() }
    viewer.send(.show(.activity))
    try await settle("Activity was never read") { viewer.panes.activity.log != nil }
    let window = ViewerWindow(controller: viewer) { _ in }
    defer {
      window.close()
      NSWindow.removeFrame(usingName: ViewerWindow.autosaveName)
    }
    window.setContentSize(Self.window)
    await Self.rest(window)
    try Self.expectListFillsTheCard(window, viewer, "as it opens")
    viewer.send(.toggleSidebar)
    await Self.rest(window)
    try Self.expectListFillsTheCard(window, viewer, "with the sidebar hidden")
    window.setContentSize(Self.zoomed)
    await Self.rest(window)
    try Self.expectListFillsTheCard(window, viewer, "zoomed with the sidebar hidden")
    window.setContentSize(Self.window)
    await Self.rest(window)
    try Self.expectListFillsTheCard(window, viewer, "back from zoomed with the sidebar hidden")
    viewer.send(.toggleSidebar)
    await Self.rest(window)
    try Self.expectListFillsTheCard(window, viewer, "with the sidebar shown again")
    window.setContentSize(Self.zoomed)
    await Self.rest(window)
    window.setContentSize(Self.narrow)
    await Self.rest(window)
    try Self.expectListFillsTheCard(window, viewer, "from zoomed to narrow")
  }

  /// Expects the list to stand exactly on the card's body: from the card's leading edge to its trailing one.
  ///
  /// - Parameters:
  ///   - window: The window.
  ///   - viewer: Its controller.
  ///   - moment: When, for the failure's words.
  /// - Throws: When the window has no content or no list.
  private static func expectListFillsTheCard(_ window: NSWindow, _ viewer: ViewerController, _ moment: String) throws {
    let content = try #require(window.contentView)
    let list = try #require(
      scrollViews(in: content)
        .max { $0.frame.width < $1.frame.width },
      "the list scrolls in the card"
    )
    let frame = list.convert(list.bounds, to: content)
    let mode = viewer.state.sidebar.mode
    let width = content.bounds.width
    #expect(frame.minX == ViewerLayout.sidebarRoom(mode) + ViewerLayout.cardLeading(mode), "its leading edge \(moment)")
    #expect(frame.width == ViewerLayout.cardWidth(windowWidth: width, mode: mode), "its width \(moment)")
  }

  /// The scroll views under `view`.
  ///
  /// - Parameter view: A view.
  /// - Returns: Its scroll views and its descendants', in tree order.
  private static func scrollViews(in view: NSView) -> [NSScrollView] {
    let own: [NSScrollView] = if let list = view as? NSScrollView { [list] } else { [] }
    return own + view.subviews.flatMap { scrollViews(in: $0) }
  }

  /// Lays `window` out and lets its animations and their completions finish, moving the held moment on past each.
  ///
  /// - Parameter window: The window.
  private static func rest(_ window: NSWindow) async {
    for _ in 0..<turns {
      window.contentView?.layoutSubtreeIfNeeded()
      await afterQueuedWork()
      Motion.Pose.hold(at: (Motion.Pose.moment ?? .zero) + settledAfter)
    }
  }

  /// A few sessions' rows, enough for the list to scroll.
  private static let log = ActivityLog(
    entries: (0..<40)
      .map { index in
        ActivityFixtures.row(
          id: "row-\(index)",
          says: "clicked Item \(index)",
          ageSeconds: Double(index * 60),
          verb: "click",
          app: index.isMultiple(of: 2) ? "Notes" : "TextEdit"
        )
      }
  )

  /// 10: seconds after which every animation has finished.
  private static let settledAfter: TimeInterval = 10
  /// 40: turns enough for a change, the layout it causes and the completions it queues, a slide's included.
  private static let turns = 40
  /// The window's own size.
  private static let window = PreviewStates.viewerSize
  /// The window zoomed to a large display.
  private static let zoomed = CGSize(width: 2560, height: 1400)
  /// A narrow window, with room for the sidebar beside the card.
  private static let narrow = CGSize(width: 1000, height: 700)
}
