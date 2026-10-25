import SwiftUI

/// One full-row navigation button with a live summary and no nested actions.
struct SettingsDisclosureRow: View {
  /// The destination's name.
  let title: String
  /// The current values or facts at that destination.
  let summary: String
  /// Opens the destination.
  let open: @MainActor () -> Void
  /// Whether the pointer is over this row.
  @State private var hovered = false

  /// The existing row geometry with a quiet navigation chevron.
  var body: some View {
    Button(action: open) {
      ListRowBody(title, detail: summary, size: .compact, fill: hovered ? .fillRowHover : .clear) {
        Image(systemName: SymbolName.next.rawValue)
          .font(Typography.symbol(.row))
          .foregroundStyle(Color.textTertiary)
          .accessibilityHidden(true)
      }
    }
    .buttonStyle(.plain)
    .onHover { hovered = $0 }
    .animation(Motion.hover(hovered), value: hovered)
    .accessibilityElement(children: .combine)
  }
}
