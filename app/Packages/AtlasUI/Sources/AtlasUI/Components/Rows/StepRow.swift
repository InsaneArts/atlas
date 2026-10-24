import SwiftUI

/// A numbered step: waiting, current (blue) or done (a green check).
struct StepRow: View {
  /// The step's number.
  let index: Int
  /// What to do.
  let title: String
  /// Where or how.
  let detail: String
  /// Where the step stands.
  let state: StepState

  var body: some View {
    HStack(spacing: Spacing.m) {
      Disc(content: disc, tone: discTone)
        .accessibilityHidden(true)
      TitleDetail(title: title, detail: detail, titleTone: state == .waiting ? .secondary : .primary, wraps: true)
      Spacer(minLength: .zero)
    }
    .padding(.horizontal, Spacing.mPlus)
    .padding(.vertical, Spacing.m)
    .cardSurface(state == .current ? .accent : .neutral)
    .accessibilityElement(children: .combine)
    .accessibilityValue(state.spokenValue)
  }

  /// A check once done, else the step's number.
  private var disc: Disc.Content {
    switch state {
    case .done: .symbol("checkmark")
    case .current, .waiting: .number(index)
    }
  }

  private var discTone: Disc.Tone {
    switch state {
    case .done: .success
    case .current: .accent
    case .waiting: .waiting
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.step)
  }

  #Preview("Interactive") {
    @Previewable @State var state = StepState.current
    InteractivePreview {
      Picker(String("State"), selection: $state) {
        Text(verbatim: "Waiting")
          .tag(StepState.waiting)
        Text(verbatim: "Current")
          .tag(StepState.current)
        Text(verbatim: "Done")
          .tag(StepState.done)
      }
    } content: {
      StepRow(
        index: 2,
        title: String("Turn its switch on"),
        detail: String("Confirm with Touch ID or your password"),
        state: state
      )
      .frame(width: SampleWidth.column)
    }
  }
#endif
