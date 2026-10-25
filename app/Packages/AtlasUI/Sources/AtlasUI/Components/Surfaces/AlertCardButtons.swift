import SwiftUI

/// The buttons, side by side or stacked, each 28 high and sharing the width. Once done they fade out under one
/// Done button that takes their place and the default's key, so the row keeps its height.
struct AlertCardButtons<ID: Hashable & Sendable>: View {
  /// What the alert says and offers.
  let state: AlertCardState<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void

  /// The component drawn from its current state.
  var body: some View {
    ZStack {
      asked
        .opacity(state.isDone ? 0 : 1)
        .allowsHitTesting(!state.isDone)
        .accessibilityHidden(state.isDone)
      if let done = state.doneButton {
        AlertCardDoneButton(button: done, send: send)
          .transition(.opacity)
      }
    }
    .animation(Motion.textSwap, value: state.isDone)
  }

  /// The asked buttons in their arrangement.
  @ViewBuilder private var asked: some View {
    switch arrangement {
    case .sideBySide:
      HStack(spacing: Spacing.s) { buttons }
    case .stacked:
      VStack(spacing: Spacing.s) { buttons }
    }
  }

  /// How the buttons sit.
  private var arrangement: AlertCard<ID>.Arrangement {
    AlertCard<ID>.arrangement(labels: state.buttons.map(\.title))
  }

  /// Each button in drawing order.
  private var buttons: some View {
    ForEach(AlertCard<ID>.ordered(state.buttons, in: arrangement)) { button in
      AlertCardButtonView(button: button, state: state, send: send)
    }
  }
}
