import SwiftUI

/// A two-line title: the statement, then its quieter tail.
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
