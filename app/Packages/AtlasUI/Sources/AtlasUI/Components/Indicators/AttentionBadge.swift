import SwiftUI

/// A small orange disc with a bold "!", set on a picture's corner to say the person is needed there. The
/// words beside the picture say why, so VoiceOver skips it.
struct AttentionBadge: View {
  /// The disc's side.
  let side: CGFloat

  /// The disc with its mark.
  var body: some View {
    Image(systemName: SymbolName.exclamation.rawValue)
      .font(Typography.font(size: side * AttentionBadgeLayout.glyphScale, weight: .heavy))
      .foregroundStyle(Color.onAttention)
      .frame(width: side, height: side)
      .background(
        Circle()
          .fill(Color.attention)
      )
      .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.m) {
      AttentionBadge(side: IconSize.sPlus)
      AttentionBadge(side: IconSize.m)
    }
    .sampleSurface()
  }
#endif
