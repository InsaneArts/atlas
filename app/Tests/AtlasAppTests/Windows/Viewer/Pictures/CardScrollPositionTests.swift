import AppKit
import AsyncSupportTesting
import SwiftUI
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Card scroll position", .serialized, AppKitState())
@MainActor
struct CardScrollPositionTests {
  @Test("scrolling partway through top picture clearance counts as scrolled, also after the picture changes ends")
  func partialTopClearanceIsNotTheTop() async throws {
    var atTop = true
    let content = { (top: Bool) in
      CardScroll { ScrollTopProbe { atTop = $0 } }
        .environment(\.miniClearance, MiniClearance(edges: top ? .top : .bottom, length: 149))
        .frame(width: 600, height: 400)
    }
    let hosting = NSHostingView(rootView: content(true))
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    window.isReleasedWhenClosed = false
    window.contentView = hosting
    defer { window.contentView = nil }
    hosting.layoutSubtreeIfNeeded()
    let list = try #require(Self.scroll(in: hosting))
    #expect(atTop)
    for top in [true, false, true] {
      hosting.rootView = content(top)
      hosting.layoutSubtreeIfNeeded()
      list.contentView.scroll(to: NSPoint(x: 0, y: 24))
      list.reflectScrolledClipView(list.contentView)
      for _ in 0..<6 {
        await afterQueuedWork()
        hosting._renderForTest(interval: .zero)
        hosting.layoutSubtreeIfNeeded()
      }
      #expect(list.contentView.bounds.minY > 0)
      #expect(!atTop, "24 points into the 149-point band is scrolled, with top clearance \(top)")
    }
  }

  private static func scroll(in view: NSView) -> NSScrollView? {
    if let scroll = view as? NSScrollView { return scroll }
    return view.subviews.lazy.compactMap { scroll(in: $0) }.first
  }
}
