import SwiftUI

/// One asked button: its own word, after a spinner while its command runs, so its width never changes;
/// VoiceOver hears the running word. Every button waits while the command runs; the one running it stays whole.
struct AlertCardButtonView<ID: Hashable & Sendable>: View {
  /// The button.
  let button: AlertCardButton<ID>
  /// What the alert says and offers.
  let state: AlertCardState<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void

  /// The component drawn from its current state.
  var body: some View {
    Button(role: button.role.buttonRole) {
      send(button.id)
    } label: {
      HStack(spacing: AlertCardLayout.markGap) {
        if state.isWorking(button) {
          Spinner(size: IconSize.xs, arc: button.role.spinnerArc)
            .transition(.opacity)
        }
        Text(state.label(of: button))
      }
      .animation(Motion.textSwap, value: state.isWorking(button))
    }
    .buttonStyle(.capsule(button.role.look, size: .compact, fill: true, isWorking: state.isWorking(button)))
    .keyboardShortcut(state.isBusy ? nil : button.role.shortcut)
    .disabled(state.isBusy)
    .accessibilityLabel(state.spokenLabel(of: button))
  }
}
