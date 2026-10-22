import SwiftUI

/// A two-line title: the statement, then its quieter tail. Each takes at most two lines, shrinking a little first;
/// what still does not fit is cut in its middle, so a long name inside keeps both its ends and the sentence its end.
struct Headline: View {
  /// How large the title is.
  enum Size: Sendable {
    /// An onboarding screen's title.
    case hero
    /// A title over the viewer's stage.
    case title
    /// The guide panel's narrow title.
    case compact
  }

  /// The statement.
  let title: String
  /// The quieter second line, if any.
  var tail: String?
  /// How large the title is.
  var size = Size.hero

  var body: some View {
    VStack(spacing: .zero) {
      Text(title)
        .foregroundStyle(Color.textPrimary)
      if let tail {
        Text(tail)
          .foregroundStyle(Color.textSecondary)
      }
    }
    .lineLimit(Typography.headlineLines)
    .minimumScaleFactor(Typography.leastHeadlineScale)
    .truncationMode(.middle)
    .font(Typography.font(style, weight: .semibold))
    .tracking(LetterSpacing.headline)
    .multilineTextAlignment(.center)
    .contentTransition(.opacity)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.isHeader)
  }

  private var style: TextStyle {
    switch size {
    case .hero: .largeTitle
    case .title: .title
    case .compact: .title2
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    TextSamples(.headline)
  }
#endif
