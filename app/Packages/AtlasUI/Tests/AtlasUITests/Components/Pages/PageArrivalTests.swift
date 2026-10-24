import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

/// A page's pieces in a window, through AppKit's hosting view, which only this test target imports: AtlasUI itself
/// never does.
@MainActor
@Suite("PageArrival")
struct PageArrivalTests {
  @Test("a page's pieces arrive once: a column that starts to scroll on the same page shows them at once")
  func overflowOnTheSamePageKeepsThePiecesShown() throws {
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: Self.size),
      styleMask: [.borderless],
      backing: .buffered,
      defer: false
    )
    let host = NSHostingView(rootView: Self.page(tall: false))
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    #expect(try Self.redness(host) < Self.shown, "the piece fades in as its page arrives")
    // A long problem appears: the column scrolls now, on the same page, which has arrived.
    host.rootView = Self.page(tall: true)
    host.layoutSubtreeIfNeeded()
    #expect(try Self.redness(host) > Self.shown, "the piece faded out to arrive again")
  }

  /// How much redder than blue the piece is drawn once shown: pure red over the blue ground reads about 0.77 in sRGB,
  /// and a piece faded out reads below 0.
  private static let shown: CGFloat = 0.5

  /// The window's size.
  private static let size = CGSize(width: 320, height: 240)

  /// A page of one red piece, short enough to fit or too tall to.
  ///
  /// - Parameter tall: Whether the piece is taller than the window.
  /// - Returns: The page.
  private static func page(tall: Bool) -> some View {
    ScrollColumn(width: size.width) {
      Rectangle()
        .fill(Color(red: 1, green: 0, blue: 0))
        .frame(height: tall ? size.height * 2 : size.height / 4)
        .pagePiece(.content)
    }
    .frame(width: size.width, height: size.height)
    .background(Color(red: 0, green: 0, blue: 1))
  }

  /// How red the middle of the top of the window is drawn, from 0 to 1, with the blue ground taken off.
  ///
  /// - Parameter view: The view.
  /// - Returns: The redness.
  /// - Throws: A failed requirement when the view cannot be drawn.
  private static func redness(_ view: NSView) throws -> CGFloat {
    let picture = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
    view.cacheDisplay(in: view.bounds, to: picture)
    let scale = CGFloat(picture.pixelsWide) / view.bounds.width
    // A third of the way down: inside the short piece, centred, and inside the tall one, scrolled to its top.
    let x = Int(view.bounds.midX * scale)
    let y = Int(view.bounds.height * 0.4 * scale)
    let colour = try #require(picture.colorAt(x: x, y: y)?.usingColorSpace(.sRGB))
    return colour.redComponent - colour.blueComponent
  }
}
