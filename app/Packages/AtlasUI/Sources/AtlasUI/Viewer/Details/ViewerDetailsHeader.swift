import SwiftUI

/// Details' header (v3 `.vdetails .hd`): its name, and Check Again as a small round ↻, a spinner in its place while
/// the check runs.
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
        if isChecking {
          Spinner(size: IconSize.xs)
        } else {
          Image(systemName: SymbolName.reload.rawValue)
        }
      }
      .buttonStyle(.capsule(.glass, size: .mini, outline: .circle))
      .help(Text(verbatim: "Check the helper again"))
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
