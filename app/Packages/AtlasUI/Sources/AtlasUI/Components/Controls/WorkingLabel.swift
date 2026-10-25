import SwiftUI

/// A button's label that shows its work without changing its width: the title is always laid out, so the button keeps
/// the size it has at rest, and while the action runs a spinner takes the title's place in the middle, the title
/// fading out under it. VoiceOver hears the working title meanwhile, such as "Connecting…". It animates its own
/// changes, since the state behind them lands from work that carries no animation; only opacity changes, so Reduce
/// Motion keeps it.
struct WorkingLabel: View {
  /// The title, which sets the width whether or not the action runs.
  let title: String
  /// Whether the button's action runs now, which shows the spinner in the title's place.
  var isWorking = false
  /// What VoiceOver says while the action runs, such as "Connecting…"; nil says the title.
  var workingTitle: String?
  /// The spinner's arc colour: the accent on glass, white on the accent.
  var arc = Color.accent
  /// The spinner's track colour, under its arc: a faint white by default, which a white button needs darker.
  var track = Color.strokeStrong

  /// What it draws, from its state.
  var body: some View {
    ZStack {
      Text(title)
        .opacity(isWorking ? 0 : 1)
      if isWorking {
        Spinner(size: IconSize.xs, arc: arc, track: track)
          .transition(.opacity)
      }
    }
    .animation(Self.animation, value: isWorking)
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(isWorking ? workingTitle ?? title : title)
  }

  /// How the label moves to its new state: the title and the spinner crossfading over ``Motion/textSwap``.
  nonisolated static var animation: Animation { Motion.textSwap }
}
