import SwiftUI

/// A caption's words on glass, led by its session's icon while sessions show: one line for the captions over the
/// main window's screen and the panel's picture alike, cut short at its end.
struct CaptionLine: View {
  /// The session's icon; nil for none.
  let icon: RowIcon?
  /// What was done, such as "Clicked Save in TextEdit".
  let text: String

  /// The icon, then the words.
  var body: some View {
    HStack(spacing: Spacing.xsPlus) {
      if let icon {
        SessionIcon(icon: icon, side: Layout.icon)
      }
      Text(text)
        .truncationMode(.tail)
    }
  }
}

/// The caption's own measures.
private enum Layout {
  /// 14: the session's icon before the words.
  static let icon = IconSize.xsPlus
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .leading, spacing: Spacing.m) {
      GlassChip(lead: .dot(.active)) {
        CaptionLine(icon: SessionSamples.claudeIcon, text: SessionSamples.claudeDoing)
      }
      GlassChip(lead: .dot(.active), size: .small) {
        CaptionLine(icon: nil, text: SessionSamples.claudeDoing)
      }
    }
    .sampleSurface()
  }
#endif
