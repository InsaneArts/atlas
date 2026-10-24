import SwiftUI

/// The helper's status in one line: the orbit, the mark, the word in its colour, then the line beside it in grey,
/// which truncates first. VoiceOver reads it as one.
struct StatusLabel: View {
  /// The status.
  let status: StatusSummary
  /// Whether the line beside the word shows; a narrow window keeps the word alone.
  var showsLine = true

  var body: some View {
    HStack(spacing: Spacing.sPlus) {
      OrbitMark(height: IconSize.xs)
      HStack(spacing: Spacing.s) {
        StatusDot(mark: status.mark, tone: status.tone)
        Text(status.label)
          .font(Typography.font(.body, weight: .semibold))
          .foregroundStyle(status.tone.wordColor)
          .fixedSize()
        if showsLine {
          Text(status.line)
            .font(Typography.font(.body))
            .foregroundStyle(Color.textTertiary)
            .truncationMode(.tail)
        }
      }
    }
    .lineLimit(1)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.updatesFrequently)
  }
}

#if DEBUG
  #Preview("Samples") {
    StatusLabelSamples()
  }
#endif
