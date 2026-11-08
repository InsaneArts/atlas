import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("CardScroll")
struct CardScrollTests {
  /// A list's rows: how many it shows, and where each stands in its window as it reports it.
  @Observable
  final class Rows {
    /// How many rows the list shows.
    var count = 2
    /// Each row's frame in the window, by its index.
    @ObservationIgnored var frames: [Int: CGRect] = [:]
  }

  /// A list of `rows` in a card's scroll view.
  struct Listing: View {
    /// The rows it shows.
    let rows: Rows

    var body: some View {
      CardScroll {
        VStack(spacing: 0) {
          ForEach(0..<rows.count, id: \.self) { index in
            Color.clear
              .frame(height: 40)
              .onGeometryChange(for: CGRect.self) { proxy in
                proxy.frame(in: .global)
              } action: { frame in
                rows.frames[index] = frame
              }
          }
        }
      }
    }
  }

  /// Turns the run loop a few times, so the list lays out and its scroll bar comes or goes.
  ///
  /// - Parameter host: The hosting view.
  static func settle(_ host: NSView) {
    for _ in 0..<4 {
      RunLoop.main
        .run(until: Date(timeIntervalSinceNow: 0.02))
      host.layoutSubtreeIfNeeded()
    }
  }

  /// The list's scroll view under `view`.
  ///
  /// - Parameter view: The hosting view.
  /// - Returns: The scroll view, if any.
  static func scrollView(in view: NSView) -> NSScrollView? {
    if let scroll = view as? NSScrollView { return scroll }
    for subview in view.subviews {
      if let scroll = scrollView(in: subview) { return scroll }
    }
    return nil
  }

  @Test("rows keep their width and place as a list grows past its height and shrinks under it")
  func rowsKeepTheirWidthAndPlaceAcrossTheScrollingThreshold() throws {
    let rows = Rows()
    let host = NSHostingView(rootView: Listing(rows: rows))
    host.frame = NSRect(x: 0, y: 0, width: 300, height: 200)
    let window = NSWindow(contentRect: host.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.contentView = host
    defer { window.contentView = nil }
    Self.settle(host)
    let short = try #require(rows.frames[0])
    let scroll = try #require(Self.scrollView(in: host))
    #expect(scroll.verticalScroller?.isHidden ?? true, "a list shorter than its height shows no scroll bar")
    #expect(short.width == 300 - CardScrollRoom.scrollerGutter(NSScroller.preferredScrollerStyle))

    rows.count = 20
    Self.settle(host)
    if NSScroller.preferredScrollerStyle == .legacy {
      #expect(scroll.verticalScroller?.isHidden == false, "a classic scroll bar shows and takes its room")
    }
    #expect(rows.frames[0] == short, "the rows neither narrow nor move as the scroll bar comes")

    rows.count = 2
    Self.settle(host)
    #expect(rows.frames[0] == short, "nor widen or move as it goes")
  }
}
