import SwiftUI

/// A two-line title: the statement, then its quieter tail. A setup screen's title is 24 points, and steps down to 20
/// when a long name inside would take it past two lines, its lines then cut in the middle at three, so a long name
/// keeps both its ends and never shrinks the type (v3 D10). The other sizes take at most two lines each, cut in the
/// middle.
struct Headline: View {
  /// How large the title is.
  enum Size: Sendable {
    /// A setup screen's title: 24 points, 20 for a long name.
    case hero
    /// A title over the viewer's stage.
    case title
    /// The permission panel's narrow title: 20 points (v3 `.ttl.side`).
    case compact
  }

  /// The statement.
  let title: String
  /// The quieter second line, if any.
  var tail: String?
  /// How large the title is.
  var size = Size.hero

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
        pair(.title3, lines: Layout.steppedLines)
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
    let lines = Layout.titleLines + (hasTail ? 1 : 0)
    return (CGFloat(lines) + Layout.spareLine) * Layout.titleLineHeight
  }
}

/// v3's D10 step down.
private enum Layout {
  /// 2: the most lines a statement takes at 24 points.
  static let titleLines = 2
  /// 3: the most lines each of statement and tail takes at 20 points, cut in the middle past them.
  static let steppedLines = 3
  /// 0.5: the part of a line the budget spares.
  static let spareLine: CGFloat = 0.5
  /// 29: one line at 24 points (``TextStyle/title``'s line height).
  static let titleLineHeight = TextStyle.title.lineHeight ?? TextStyle.title.pointSize
}

#if DEBUG
  #Preview("Samples") {
    TextSamples(.headline)
  }
#endif
