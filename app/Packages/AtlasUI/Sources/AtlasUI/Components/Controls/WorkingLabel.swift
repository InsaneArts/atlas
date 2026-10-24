import SwiftUI

/// A button's label that shows its work (v3 G7, G9): a spinner before the title while the button's action runs, the
/// title morphing as it changes, such as Connect to Connecting…. It animates its own changes, since the state behind
/// them lands from work that carries no animation; under Reduce Motion the spinner and the words only crossfade.
struct WorkingLabel: View {
  /// The title.
  let title: String
  /// Whether the button's action runs now, which shows the spinner.
  var isWorking = false
  /// The spinner's arc colour: the accent on glass, white on the accent.
  var arc = Color.accent
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// What it draws, from its state.
  var body: some View {
    HStack(spacing: Layout.gap) {
      if isWorking {
        Spinner(size: IconSize.xs, arc: arc)
          .transition(.opacity)
      }
      Text(title)
        .contentTransition(Motion.contentTransition(.interpolate, reduceMotion: reduceMotion))
    }
    .animation(Self.animation(reduceMotion: reduceMotion), value: isWorking)
    .animation(Self.animation(reduceMotion: reduceMotion), value: title)
  }

  /// How the label moves to its new state: ``Motion/state``, or the short fade ``Motion/reduced`` under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The animation.
  nonisolated static func animation(reduceMotion: Bool) -> Animation {
    Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced)
  }
}

/// The label's own measures (v3 `.cap .spin`).
private enum Layout {
  /// 7: between the spinner and the title.
  static let gap: CGFloat = 7
}
