import SwiftUI

/// The one button a done alert shows: a check, then its word, such as "Done", in the default's accent, which
/// Return presses to close the alert now.
struct AlertCardDoneButton<ID: Hashable & Sendable>: View {
  /// The button, which sends the way out.
  let button: AlertCardButton<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void

  /// The component drawn from its current state.
  var body: some View {
    Button {
      send(button.id)
    } label: {
      HStack(spacing: AlertCardLayout.markGap) {
        Image(systemName: SymbolName.passed.rawValue)
          .symbolRenderingMode(.palette)
          .foregroundStyle(Color.accent, Color.onAccent)
          .font(Typography.symbol(.control, weight: .semibold))
          .accessibilityHidden(true)
        Text(button.title)
      }
    }
    .buttonStyle(.capsule(button.role.look, size: .compact, fill: true))
    .keyboardShortcut(.defaultAction)
  }
}
