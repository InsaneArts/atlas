import SwiftUI

/// Details' header: its name, and Check Again as a small round ↻. While the check runs the ↻ stays as it is and takes
/// no press, so nothing flashes for a check that ends in a moment; the controller holds the check for at least
/// ``Motion/Duration/leastWork`` so its answer reads as checked.
struct ViewerDetailsHeader: View {
  /// Whether Check Again is reading the helper now.
  let isChecking: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The name, and Check Again at the trailing edge.
  var body: some View {
    HStack(spacing: Spacing.s) {
      Text(verbatim: "Details")
        .textStyle(.body, weight: .semibold)
        .foregroundStyle(Color.textPrimary)
        .accessibilityAddTraits(.isHeader)
      Spacer(minLength: .zero)
      Button {
        send(.checkAgain)
      } label: {
        Image(systemName: SymbolName.reload.rawValue)
      }
      .buttonStyle(.capsule(.glass, size: .mini, outline: .circle, isWorking: isChecking))
      .disabled(isChecking)
      .help(Text(verbatim: "Check Again"))
      .accessibilityLabel(Text(verbatim: "Check Again"))
    }
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var isChecking = false
    InteractivePreview {
      Toggle(String("Checking"), isOn: $isChecking)
    } content: {
      ViewerDetailsHeader(isChecking: isChecking) { _ in isChecking.toggle() }
        .frame(width: ViewerLayout.detailsWidth)
    }
  }
#endif
