import SwiftUI

/// One full-row navigation button with a live summary and no nested actions.
struct SettingsDisclosureRow: View {
  /// The destination's name.
  let title: String
  /// The current values or facts at that destination.
  let summary: String
  /// The detail whose link regains focus when history returns to this root.
  var destination: SettingsDestination? = nil
  /// Opens the destination.
  let open: @MainActor () -> Void
  /// Whether the pointer is over this row.
  @State private var hovered = false
  @Environment(\.settingsReveal) private var reveal
  @FocusState private var focused: Bool

  /// The existing row geometry with a quiet navigation chevron.
  var body: some View {
    Button(action: open) {
      ListRowBody(title, detail: summary, swapsDetail: true, size: .compact, fill: hovered ? .fillRowHover : .clear) {
        Image(systemName: SymbolName.next.rawValue)
          .font(Typography.symbol(.row))
          .foregroundStyle(Color.textTertiary)
          .accessibilityHidden(true)
      }
    }
    .buttonStyle(.plain)
    .focusable()
    .focused($focused)
    .focusEffectDisabled()
    .overlay {
      RoundedRectangle(cornerRadius: Radius.small)
        .strokeBorder(focused ? Color.strokeFocus : Color.clear, lineWidth: Stroke.regular)
        .allowsHitTesting(false)
    }
    .onChange(of: reveal) { _, request in
      if let destination, request?.target == .disclosure(destination) { focused = true }
    }
    .onKeyPress(.return) {
      guard focused else { return .ignored }
      open()
      return .handled
    }
    .onHover { hovered = $0 }
    .animation(Motion.hover(hovered), value: hovered)
    .accessibilityElement(children: .combine)
  }
}
