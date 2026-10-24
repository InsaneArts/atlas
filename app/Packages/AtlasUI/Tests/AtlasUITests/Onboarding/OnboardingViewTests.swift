#if DEBUG
  import AppKit
  import SwiftUI
  import Testing

  @testable import AtlasUI

  /// The setup window's view in a window, through AppKit's hosting view, which only this test target imports: AtlasUI
  /// itself never does.
  @MainActor
  @Suite("OnboardingView")
  struct OnboardingViewTests {
    @Test("the footer stays in place, drawn the same, while the screen above it slides to the next")
    func theFooterStaysWhileTheScreenSlides() throws {
      let window = NSWindow(
        contentRect: NSRect(origin: .zero, size: OnboardingLayout.card),
        styleMask: [.borderless],
        backing: .buffered,
        defer: false
      )
      let first = PreviewStates.onboardingChoose
      // The next screen under the very same footer, so only the screen above it has anything to change.
      var next = first
      next.content = PreviewStates.onboardingWelcome.content
      next.direction = .forward
      let host = NSHostingView(rootView: Self.window(first))
      window.contentView = host
      host.layoutSubtreeIfNeeded()
      let before = try Self.picture(host)
      host.rootView = Self.window(next)
      host.layoutSubtreeIfNeeded()
      // Part way through the slide, which takes about half a second.
      RunLoop.main
        .run(until: Date(timeIntervalSinceNow: Self.partWay))
      host.layoutSubtreeIfNeeded()
      let during = try Self.picture(host)
      let footer = OnboardingLayout.card.height - OnboardingLayout.footerHeight
      #expect(Self.differs(before, during, rows: 0..<footer), "the screen above never moved")
      #expect(
        !Self.differs(before, during, rows: footer..<OnboardingLayout.card.height),
        "the footer moved or redrew with the screen"
      )
    }

    /// How long into the slide the window is drawn again: past the new screen's wait to come in, and short of its end.
    private static let partWay: TimeInterval = 0.2

    /// How far apart the points compared are, in points.
    private static let step: CGFloat = 4

    /// The window's content as `state` shows it, drawn settled, as a render draws it, so a screen's pieces show at
    /// once and only the slide moves.
    ///
    /// - Parameter state: What the window shows.
    /// - Returns: The content.
    private static func window(_ state: OnboardingState) -> some View {
      OnboardingView(state: state, form: .constant(PreviewStates.onboardingForm)) { _ in }
        .environment(\.animatesArrivals, false)
    }

    /// The view as it draws now.
    ///
    /// - Parameter view: The view.
    /// - Returns: Its drawing.
    /// - Throws: A failed requirement when it cannot be drawn.
    private static func picture(_ view: NSView) throws -> NSBitmapImageRep {
      let picture = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
      view.cacheDisplay(in: view.bounds, to: picture)
      return picture
    }

    /// Whether two drawings of the window differ anywhere in a band of it, compared every ``step`` points.
    ///
    /// - Parameters:
    ///   - one: A drawing.
    ///   - other: Another drawing of the same window.
    ///   - rows: The band, in points from the window's top.
    /// - Returns: Whether they differ there.
    private static func differs(_ one: NSBitmapImageRep, _ other: NSBitmapImageRep, rows: Range<CGFloat>) -> Bool {
      let scale = CGFloat(one.pixelsWide) / OnboardingLayout.card.width
      let xs = stride(from: CGFloat.zero, to: OnboardingLayout.card.width, by: step)
      let ys = stride(from: rows.lowerBound, to: rows.upperBound, by: step)
      return ys.contains { y in
        xs.contains { x in
          one.colorAt(x: Int(x * scale), y: Int(y * scale)) != other.colorAt(x: Int(x * scale), y: Int(y * scale))
        }
      }
    }
  }
#endif
