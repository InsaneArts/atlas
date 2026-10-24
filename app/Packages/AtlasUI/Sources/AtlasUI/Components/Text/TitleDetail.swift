import SwiftUI

/// A title over what it means, left-aligned, as cards and rows set them.
struct TitleDetail: View {
  /// How large the pair is.
  enum Size: Sendable {
    /// A row's title and detail.
    case regular
    /// A card's in a narrow window.
    case compact
  }

  /// The title.
  let title: String
  /// What it means.
  let detail: String
  /// How large the pair is.
  var size = Size.regular
  /// How the title is painted: primary, or secondary for a step that is not reached yet.
  var titleTone = TextTone.primary
  /// Whether the title wraps onto as many lines as it needs, rather than being cut.
  var titleWraps = false
  /// Whether the detail wraps onto as many lines as it needs.
  var wraps = false

  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.half) {
      Text(title)
        .font(Typography.font(titleStyle, weight: .semibold))
        .foregroundStyle(titleTone.color)
        .fixedSize(horizontal: false, vertical: titleWraps)
      Text(detail)
        .font(Typography.font(detailStyle))
        .foregroundStyle(Color.textSecondary)
        .fixedSize(horizontal: false, vertical: wraps)
    }
  }

  private var titleStyle: TextStyle {
    switch size {
    case .regular: .body
    case .compact: .callout
    }
  }

  private var detailStyle: TextStyle {
    switch size {
    case .regular: .callout
    case .compact: .caption
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    TextSamples(.titleDetail)
  }
#endif
