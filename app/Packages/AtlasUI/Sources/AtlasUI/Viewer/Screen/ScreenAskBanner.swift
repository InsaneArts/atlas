import SwiftUI

/// The question Atlas asks the person, over the viewer's Screen while they watch the helper: the agent's mark, the
/// question over who asks and how long it has waited, and its answers in one row, Allow for This Task in the accent at
/// the trailing end. Opaque and dark in either appearance, as the screen it floats over is, with the chips' fine edge
/// and soft shadow. Where the row doesn't fit, the answers go under the words.
struct ScreenAskBanner: View {
  /// The question.
  let ask: AskCardState
  /// Receives the answer.
  let send: @MainActor (ViewerEvent) -> Void

  /// The banner, in one row where it fits.
  var body: some View {
    ViewThatFits(in: .horizontal) {
      HStack(spacing: AskBannerLayout.gap) {
        lead
          .frame(minWidth: AskBannerLayout.leadInRow, idealWidth: AskBannerLayout.leadInRow, maxWidth: .infinity)
        answers
      }
      VStack(alignment: .leading, spacing: AskBannerLayout.gap) {
        lead
        answers
          .frame(maxWidth: .infinity, alignment: .trailing)
      }
    }
    .padding(AskBannerLayout.inset)
    .frame(maxWidth: AskBannerLayout.width)
    .background(shape.fill(Color.fillOverPicture))
    .overlay(shape.strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline))
    .background { ChipShadow(shape: shape) }
    .environment(\.colorScheme, .dark)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(ask.label)
  }

  /// The agent's mark and the words.
  private var lead: some View {
    HStack(spacing: AskBannerLayout.gap) {
      SessionIcon(icon: ask.icon, side: AskCardLayout.whoIcon)
      VStack(alignment: .leading, spacing: AskBannerLayout.lineGap) {
        Text("\(ask.title)?")
          .font(Typography.font(.body, weight: .semibold))
          .foregroundStyle(Color.textOverPicture)
        Text("\(ask.who) · waiting \(ask.waited)")
          .font(Typography.font(.caption))
          .monospacedDigit()
          .foregroundStyle(Color.markBubbleDim)
      }
      .lineLimit(1)
      .truncationMode(.tail)
      .frame(maxWidth: .infinity, alignment: .leading)
    }
  }

  /// The answers, Deny first and the main one last.
  private var answers: some View {
    HStack(spacing: AskBannerLayout.gap) {
      ForEach(ask.choices.reversed(), id: \.self) { choice in
        Button(choice.title) { send(.answerAsk(id: ask.id, choice: choice)) }
          .buttonStyle(.capsule(choice == ask.main && choice != .deny ? .accent : .secondary, size: .compact))
      }
    }
    .fixedSize()
  }

  /// The banner's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: AskBannerLayout.corner, style: .continuous)
  }
}

#if DEBUG
  #Preview("Over the Screen") {
    AskSamples(.banner)
  }
#endif
