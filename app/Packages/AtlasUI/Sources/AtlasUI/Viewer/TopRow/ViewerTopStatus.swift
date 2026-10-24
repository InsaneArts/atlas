import SwiftUI

/// The helper's status at the top row's leading edge (v3 V1): its mark, its word in its tone (blue while this window
/// holds control), and its line in grey, cut short first and gone once too little of it would show. A new word or line
/// replaces the old with a blur, a fade under Reduce Motion. VoiceOver reads it as one.
struct ViewerTopStatus: View {
  /// The status.
  let status: StatusSummary
  /// Whether the status asks the row for room for its line; otherwise the line takes only the room left.
  let reservesLine: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The mark, the word and the line.
  var body: some View {
    HStack(spacing: Spacing.s) {
      StatusDot(mark: status.mark, tone: status.tone, size: .large)
      Text(status.label)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(status.tone.wordColor)
        .fixedSize()
        .id(status.label)
        .transition(changing)
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
      .frame(minWidth: .zero, idealWidth: lineRoom, maxWidth: .infinity, alignment: .leading)
      .id(status.line)
      .transition(changing)
    }
    .lineLimit(1)
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: status)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.updatesFrequently)
  }

  /// The room the line asks the row for.
  private var lineRoom: CGFloat {
    reservesLine ? ViewerLayout.statusLineRoom : .zero
  }

  /// How a word gives way to the next: a blur, or a fade under Reduce Motion.
  private var changing: AnyTransition {
    Motion.transition(AnyTransition(.blurReplace(.downUp)), reduceMotion: reduceMotion)
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
