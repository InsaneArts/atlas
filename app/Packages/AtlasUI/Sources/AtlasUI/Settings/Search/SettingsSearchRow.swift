import SwiftUI

/// A result is one full-row navigation button, with no executable control inside it.
struct SettingsSearchRow: View {
  /// The local presentation metadata.
  let entry: SettingsSearchEntry
  /// Whether arrows selected this result.
  let selected: Bool
  /// Keeps selection aligned when Tab reaches this row.
  let select: @MainActor () -> Void
  /// Opens its location, never its command.
  let open: @MainActor () -> Void
  /// Whether keyboard navigation reached the button itself.
  @FocusState private var focused: Bool

  /// The title, owning-page breadcrumb and brief explanation share disclosure-row geometry.
  var body: some View {
    Button(action: open) {
      HStack(spacing: Spacing.m) {
        VStack(alignment: .leading, spacing: Spacing.half) {
          Text(entry.title)
            .textStyle(.body)
            .foregroundStyle(Color.textPrimary)
          Text(entry.breadcrumb)
            .textStyle(.caption)
            .foregroundStyle(Color.textSecondary)
          Text(entry.summary)
            .textStyle(.caption)
            .foregroundStyle(Color.textSecondary)
        }
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: .infinity, alignment: .leading)
        Image(systemName: SymbolName.next.rawValue)
          .font(Typography.symbol(.row))
          .foregroundStyle(Color.textTertiary)
          .accessibilityHidden(true)
      }
      .padding(Spacing.m)
      .frame(minHeight: SettingsLayout.searchResultHeight)
      .contentShape(Rectangle())
      .background(selected ? Color.fillControlRaised : Color.clear)
    }
    .buttonStyle(.plain)
    .focused($focused)
    .onChange(of: focused) { _, focused in if focused { select() } }
    .overlay {
      RoundedRectangle(cornerRadius: Radius.small)
        .strokeBorder(focused ? Color.strokeFocus : Color.clear, lineWidth: Stroke.regular)
        .allowsHitTesting(false)
    }
    .accessibilityElement(children: .combine)
    .accessibilityValue(selected ? "Selected" : "")
  }
}
