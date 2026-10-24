import SwiftUI

/// A button's label that shows its work (v3 G7, G9): a spinner before the title while the button's action runs, the
/// title morphing as it changes, such as Connect to Connecting….
struct WorkingLabel: View {
  /// The title.
  let title: String
  /// Whether the button's action runs now, which shows the spinner.
  var isWorking = false
  /// The spinner's arc colour: the accent on glass, white on the accent.
  var arc = Color.accent

  /// What it draws, from its state.
  var body: some View {
    HStack(spacing: Layout.gap) {
      if isWorking {
        Spinner(size: IconSize.xs, arc: arc)
          .transition(.opacity)
      }
      Text(title)
        .contentTransition(.interpolate)
    }
  }
}

/// The label's own measures (v3 `.cap .spin`).
private enum Layout {
  /// 7: between the spinner and the title.
  static let gap: CGFloat = 7
}
