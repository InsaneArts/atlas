import AppKit
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Viewer visual regressions", .serialized, AppKitState())
@MainActor
struct ViewerVisualRegressionTests {
  @Test("all Activity filter labels and the entire capsule fit a 320 point viewer")
  func narrowFiltersRemainWhole() throws {
    var frames: [String: CGRect] = [:]
    let report = LayoutFrameReport { frames[$0] = $1 }
    let activity = try #require(
      { if case .activity(let value) = PreviewStates.viewerActivity.pane { value } else { nil } }()
    )
    let width = 320 - ViewerLayout.cardInset * 2
    let hosting = NSHostingView(
      rootView: ActivityListContent(state: activity, send: { _ in })
        .environment(\.layoutFrameReport, report)
        .frame(width: width, height: 900)
    )
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: width, height: 900),
      styleMask: .borderless,
      backing: .buffered,
      defer: false
    )
    window.contentView = hosting
    defer { window.contentView = nil }
    hosting.layoutSubtreeIfNeeded()
    hosting._renderForTest(interval: .zero)
    let filter = try #require(frames["activity filters"])
    #expect(filter.width > 0)
    #expect(filter.minX >= ViewerLayout.swapInset)
    #expect(filter.maxX <= width - ViewerLayout.swapInset)
  }
}
