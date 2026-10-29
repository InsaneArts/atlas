import SwiftUI

/// The mark that leads a line of what was done: a glowing blue dot for what runs now, a blue dot for the person's
/// own, an orange warning sign for what did not work, a quiet ring crossed by a dash for what was not sent, and a
/// quiet dot for the rest. VoiceOver reads the line, not the mark.
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
        .foregroundStyle(Color.onAttention, Color.attention)
    case .quiet:
      Circle()
        .fill(Color.strokePressed)
        .frame(width: IconSize.xxs, height: IconSize.xxs)
    case .notSent:
      Circle()
        .strokeBorder(Color.textTertiary, lineWidth: Stroke.regular)
        .overlay {
          Capsule()
            .fill(Color.textTertiary)
            .frame(width: Layout.dash, height: Stroke.regular)
        }
        .frame(width: IconSize.xs, height: IconSize.xs)
    }
  }
}

/// The mark's own measures.
private enum Layout {
  /// 6: the dash across the not-sent ring, half its side.
  static let dash = IconSize.xs / 2
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.mPlus) {
      ActivityMark(kind: .live)
      ActivityMark(kind: .person)
      ActivityMark(kind: .failed)
      ActivityMark(kind: .quiet)
      ActivityMark(kind: .notSent)
    }
    .sampleSurface()
  }
#endif
