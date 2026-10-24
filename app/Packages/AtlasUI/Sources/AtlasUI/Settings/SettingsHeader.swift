import SwiftUI

/// Stationary history controls and the current page's authoritative visible title.
struct SettingsHeader: View {
  /// The selected pane's name.
  let title: String
  /// Whether an earlier visit exists.
  let canGoBack: Bool
  /// Whether a later visit exists.
  let canGoForward: Bool
  /// Receives history navigation.
  let send: @MainActor (SettingsEvent) -> Void

  /// History followed by the title, aligned with the readable content column.
  var body: some View {
    HStack(spacing: SettingsLayout.headerControlGap) {
      HStack(spacing: .zero) {
        Button("Back", systemImage: SymbolName.back.rawValue) { send(.back) }
          .labelStyle(.iconOnly)
          .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
          .disabled(!canGoBack)
        Rectangle()
          .fill(Color.separator)
          .frame(width: Stroke.hairline, height: Spacing.m)
          .accessibilityHidden(true)
        Button("Forward", systemImage: SymbolName.next.rawValue) { send(.forward) }
          .labelStyle(.iconOnly)
          .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
          .disabled(!canGoForward)
      }
      Text(title)
        .textStyle(.title)
        .foregroundStyle(Color.textPrimary)
        .swapsText(on: title)
        .accessibilityAddTraits(.isHeader)
      Spacer(minLength: .zero)
    }
    .frame(maxWidth: SettingsLayout.contentMaximumWidth, alignment: .leading)
    .frame(maxWidth: .infinity)
    .padding(.horizontal, SettingsLayout.pageSideInset)
    .padding(.top, SettingsLayout.headerTopInset)
    .frame(height: SettingsLayout.headerHeight, alignment: .top)
  }
}
