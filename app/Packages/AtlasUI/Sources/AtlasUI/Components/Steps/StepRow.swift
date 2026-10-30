import SwiftUI

/// One step of a ``StepList``: its disc, a medium title over its detail, and the rail that joins it to
/// the steps above and below. The step to do now carries the only colour, an accent wash; a waiting or finished step
/// steps back to grey, a finished one with its green check. As a step finishes, the rail below it turns green, and the
/// step it leads to lights up a moment later.
struct StepRow: View {
  /// The step's number.
  let index: Int
  /// What to do.
  let title: String
  /// Where or how.
  let detail: String
  /// Where the step stands.
  let state: StepState
  /// Where the step above stands, which colours the rail joining them; nil for the first step.
  var above: StepState?
  /// Whether a step follows, which the rail runs down to.
  var continues = false
  @Environment(\.drawsListSeparator) private var drawsSeparator
  @Environment(\.motionReduced) private var reduceMotion

  /// The disc and the words, on the current step's wash, with the rail through the disc's column.
  var body: some View {
    HStack(spacing: Spacing.m) {
      StepDisc(index: index, state: state)
      VStack(alignment: .leading, spacing: Spacing.hairline) {
        Text(title)
          .textStyle(.body, weight: .medium)
          .foregroundStyle(state == .current ? Color.textPrimary : Color.textSecondary)
        Text(detail)
          .textStyle(.callout)
          .foregroundStyle(Color.textSecondary)
      }
      .lineLimit(StepRowLayout.lines)
      .truncationMode(.middle)
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .padding(.horizontal, Spacing.mPlus)
    .padding(.vertical, Spacing.sPlus)
    .frame(minHeight: StepRowLayout.height)
    .background(state == .current ? Color.accentWash : .clear)
    .overlay(alignment: .leading) { rail }
    .overlay(alignment: .top) {
      if drawsSeparator {
        SeparatorLine()
          .padding(.leading, StepRowLayout.separatorInset)
          .padding(.trailing, Spacing.mPlus)
      }
    }
    .animation(Self.animation(for: state, reduceMotion: reduceMotion), value: state)
    // The rail from the step above turns green as that step finishes, at once.
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: above)
    .accessibilityElement(children: .combine)
    .accessibilityValue(state.spokenValue)
  }

  /// The rail through the disc's column: up to the step above, if any, and down to the step below, if any, each
  /// stopping at the disc.
  private var rail: some View {
    VStack(spacing: .zero) {
      Rectangle()
        .fill(above.map(Self.railColor) ?? .clear)
      Color.clear
        .frame(height: StepDisc.side)
      Rectangle()
        .fill(continues ? Self.railColor(state) : .clear)
    }
    .frame(width: Stroke.emphasis)
    .padding(.leading, StepRowLayout.railInset)
    .accessibilityHidden(true)
  }

  /// The rail's colour below a step: green once the step is done, faint until then.
  ///
  /// - Parameter state: Where the step above the rail stands.
  /// - Returns: The rail's colour.
  nonisolated static func railColor(_ state: StepState) -> Color {
    state == .done ? .strokeRailDone : .strokeRail
  }

  /// How a step moves to a new state: a step lighting up waits ``Motion/Duration/step`` for the step before it to
  /// finish first (``Motion/stepLight``); every other change moves at once (``Motion/state``). Under Reduce Motion
  /// every change is a short fade, at once.
  ///
  /// - Parameters:
  ///   - state: The state the step moves to.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  nonisolated static func animation(for state: StepState, reduceMotion: Bool) -> Animation {
    let moving = state == .current ? Motion.stepLight : Motion.state
    return Motion.animation(moving, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.step)
  }
#endif
