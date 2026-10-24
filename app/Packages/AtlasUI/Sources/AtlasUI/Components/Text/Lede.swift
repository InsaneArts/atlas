import SwiftUI

/// The sentence or two under a title.
struct Lede: View {
  /// The sentences.
  let text: String

  /// The paragraph, centred in grey under a title.
  var body: some View {
    Text(text)
      .font(Typography.font(.body))
      .lineSpacing(Spacing.xs)
      .foregroundStyle(Color.textSecondary)
      .multilineTextAlignment(.center)
      .frame(maxWidth: Layout.width)
      .fixedSize(horizontal: false, vertical: true)
  }
}

/// The lede's measure.
private enum Layout {
  /// 450: the widest a lede runs, so it reads as a paragraph under its title.
  static let width: CGFloat = 450
}

#if DEBUG
  #Preview("Samples") {
    TextSamples(.lede)
  }
#endif
