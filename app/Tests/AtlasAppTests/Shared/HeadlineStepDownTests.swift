import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

/// Here, not in AtlasUI's tests, because measuring laid-out text needs AppKit, which AtlasUI never imports.
@MainActor
@Suite("Headline step down")
struct HeadlineStepDownTests {
  /// The second line every case shares.
  private static let tail = "Just this once, to set Atlas up there."

  @Test("a long account name steps the title down rather than shrinking it")
  func longNameStepsDown() {
    let short = "Now log in to the Atlas account."
    let long = "Now log in to Maria Fernanda Montgomery-Castellanos de la Vega y Rodríguez-Hernández del Valle."
    #expect(height(Headline(title: short, tail: Self.tail)) == height(lines(short, .title, limit: nil)))
    let stepped = height(Headline(title: long, tail: Self.tail))
    #expect(stepped == height(lines(long, .title3, limit: 3)), "20 points, in at most three lines each")
    #expect(stepped != height(lines(long, .title, limit: 2)), "never kept at 24 in two lines, as shrinking would")
    #expect(height(lines(long, .title, limit: nil)) > height(lines(long, .title, limit: 2)), "the name needs a third")
  }

  /// The statement over the tail in one style, as a measure.
  ///
  /// - Parameters:
  ///   - title: The statement.
  ///   - style: The style.
  ///   - limit: The most lines each takes, or nil for as many as it needs.
  /// - Returns: The two lines.
  private func lines(_ title: String, _ style: TextStyle, limit: Int?) -> some View {
    VStack(spacing: .zero) {
      Text(title)
      Text(Self.tail)
    }
    .textStyle(style)
    .lineLimit(limit)
    .truncationMode(.middle)
    .multilineTextAlignment(.center)
  }

  /// How tall a view lays out in a setup screen's column.
  ///
  /// - Parameter view: The view.
  /// - Returns: Its height in points.
  private func height(_ view: some View) -> CGFloat {
    NSHostingView(rootView: view.frame(width: OnboardingLayout.column))
      .fittingSize
      .height
  }
}
