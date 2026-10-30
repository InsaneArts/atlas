import SwiftUI

/// A two-line title: the statement, then its quieter tail. A setup screen's title is 24 points, and steps
/// down to 20 when a long name inside would take it past two lines, its lines then cut in the middle at
/// three, so a long name keeps both its ends and never shrinks the type. The other sizes take at most two
/// lines each, cut in the middle.
struct Headline: View {
  /// How large the title is.
  enum Size: Sendable {
    /// A setup screen's title: 24 points, 20 for a long name.
    case hero
    /// A title over the viewer's stage.
    case title
    /// The permission panel's narrow title: 20 points.
    case compact
  }

  /// The statement.
  let title: String
  /// The quieter second line, if any.
  var tail: String?
  /// How large the title is.
  var size = Size.hero

  /// The title's lines, centred and read as one header.
  var body: some View {
    lines
      .multilineTextAlignment(.center)
      .contentTransition(.opacity)
      .accessibilityElement(children: .combine)
      .accessibilityAddTraits(.isHeader)
  }

  /// The two lines at the title's size.
  @ViewBuilder private var lines: some View {
    switch size {
    case .hero: stepping
    case .title: fixed(.title)
    case .compact: fixed(.title3)
    }
  }

  /// At ``TextStyle/title`` while the statement takes at most two lines, else at ``TextStyle/title3``, each line cut
  /// in the middle at three. The step down is chosen by the height the larger lines take, so neither is ever shrunk.
  private var stepping: some View {
    HeightBudget(height: Self.budget(hasTail: tail != nil)) {
      ViewThatFits(in: .vertical) {
        pair(.title, lines: nil)
        pair(.title3, lines: HeadlineLayout.steppedLines)
          // Its own height, past the budget, for a name that takes all three lines.
          .fixedSize(horizontal: false, vertical: true)
      }
    }
  }

  /// The statement over its tail in a style of the scale.
  ///
  /// - Parameters:
  ///   - style: The style.
  ///   - lines: The most lines each takes, or nil for as many as it needs.
  /// - Returns: The two lines.
  private func pair(_ style: TextStyle, lines: Int?) -> some View {
    statement
      .textStyle(style)
      .lineLimit(lines)
      .truncationMode(.middle)
  }

  /// The statement over its tail at a fixed size, each in at most two lines.
  ///
  /// - Parameter style: The size.
  /// - Returns: The two lines.
  private func fixed(_ style: TextStyle) -> some View {
    statement
      .lineLimit(Typography.headlineLines)
      .truncationMode(.middle)
      .font(Typography.font(style, weight: .semibold))
      .tracking(LetterSpacing.headline)
  }

  /// The statement over its tail, in their colours.
  private var statement: some View {
    VStack(spacing: .zero) {
      Text(title)
        .foregroundStyle(Color.textPrimary)
      if let tail {
        Text(tail)
          .foregroundStyle(Color.textSecondary)
      }
    }
  }

  /// The height a setup screen's title has at 24 points before it steps down: two lines of statement, and one more
  /// for a tail, with half a line to spare so the measure never trips on a fraction of a point, and never a whole
  /// line more.
  ///
  /// - Parameter hasTail: Whether the title has a tail.
  /// - Returns: The height in points.
  nonisolated static func budget(hasTail: Bool) -> CGFloat {
    let lines = HeadlineLayout.titleLines + (hasTail ? 1 : 0)
    return (CGFloat(lines) + HeadlineLayout.spareLine) * HeadlineLayout.titleLineHeight
  }
}

#if DEBUG
  #Preview("Samples") {
    TextSamples(.headline)
  }
#endif
