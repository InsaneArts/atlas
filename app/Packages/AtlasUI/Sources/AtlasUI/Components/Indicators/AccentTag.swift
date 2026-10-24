import SwiftUI

/// A short word on a blue-tinted capsule beside a name, such as an app's "In front".
struct AccentTag: View {
  /// The word.
  let text: String

  var body: some View {
    Text(text)
      .font(Typography.font(.caption, weight: .semibold))
      .foregroundStyle(Color.textLink)
      .lineLimit(1)
      .padding(.vertical, Spacing.xs)
      .padding(.horizontal, Spacing.s)
      .background(
        Capsule()
          .fill(
            Color.accent
              .opacity(Opacity.tintStrong)
          )
      )
  }
}

#if DEBUG
  #Preview("Samples") {
    AccentTag(text: String("In front"))
      .sampleSurface()
  }
#endif
