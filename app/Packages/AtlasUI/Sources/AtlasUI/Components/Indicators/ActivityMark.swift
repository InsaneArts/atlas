import SwiftUI

/// The mark that leads a line of what was done: a glowing blue dot for what runs now, a blue dot for the person's
/// own, an orange warning sign for what did not work, and a quiet dot for the rest. VoiceOver reads the line, not the
/// mark.
struct ActivityMark: View {
  /// Which mark it draws.
  let kind: ActivityMarkKind

  /// The mark for the line's kind, hidden from VoiceOver.
  var body: some View {
    mark
      .accessibilityHidden(true)
  }

  /// The mark itself.
  @ViewBuilder private var mark: some View {
    switch kind {
    case .live:
      StatusDot(mark: .dot, tone: .active)
    case .person:
      StatusDot(mark: .small, tone: .active)
    case .failed:
      Image(systemName: "exclamationmark.triangle.fill")
        .font(Typography.font(.callout))
        .symbolRenderingMode(.palette)
        .foregroundStyle(Color.ground, Color.attention)
    case .quiet:
      Circle()
        .fill(Color.strokePressed)
        .frame(width: IconSize.xxs, height: IconSize.xxs)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.mPlus) {
      ActivityMark(kind: .live)
      ActivityMark(kind: .person)
      ActivityMark(kind: .failed)
      ActivityMark(kind: .quiet)
    }
    .sampleSurface()
  }
#endif
