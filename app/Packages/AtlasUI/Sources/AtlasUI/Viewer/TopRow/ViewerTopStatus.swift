import SwiftUI

/// The helper's status at the top row's leading edge: its mark, its word in its tone (blue while this window
/// holds control), and in the row's widest layout its line in grey, cut short first and gone once too little
/// of it would show; every narrower layout has no line at all, so the line goes before the tabs' words do. A
/// new word swaps in place by opacity and the dot eases to its new colour; the line, which changes every
/// second or so while an agent works, changes with no transition at all. VoiceOver reads it as one.
struct ViewerTopStatus: View {
  /// The status.
  let status: StatusSummary
  /// Whether the line shows, asking the row for room for it: in the widest layout alone.
  let reservesLine: Bool

  /// The mark, the word and the line.
  var body: some View {
    HStack(spacing: Spacing.s) {
      StatusDot(mark: status.mark, tone: status.tone, size: .large)
        .animation(Motion.fade, value: status.tone)
      Text(status.label)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(status.tone.wordColor)
        .fixedSize()
        .swapsText(on: status.label)
      if reservesLine {
        line
      }
    }
    .lineLimit(1)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.updatesFrequently)
  }

  /// The line, cut short first and gone once too little of it would show.
  private var line: some View {
    ViewThatFits(in: .horizontal) {
      Text(status.line)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textSecondary)
        .truncationMode(.tail)
        .frame(
          minWidth: ViewerLayout.statusLineLeast,
          idealWidth: ViewerLayout.statusLineLeast,
          maxWidth: .infinity,
          alignment: .leading
        )
      Color.clear
        .frame(width: .zero, height: .zero)
    }
    .frame(minWidth: .zero, idealWidth: ViewerLayout.statusLineRoom, maxWidth: .infinity, alignment: .leading)
  }
}

#if DEBUG
  #Preview("Working") {
    ViewerTopStatus(status: PreviewStates.viewerWorking.top.status, reservesLine: true)
      .frame(width: SampleWidth.column)
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    @Previewable @State var reservesLine = true
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
      Toggle(String("Room for the line"), isOn: $reservesLine)
    } content: {
      ViewerTopStatus(status: state.top.status, reservesLine: reservesLine)
        .frame(width: SampleWidth.column)
    }
  }
#endif
