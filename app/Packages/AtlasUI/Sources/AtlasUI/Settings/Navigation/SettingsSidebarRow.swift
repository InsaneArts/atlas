import SwiftUI

/// One keyboard-accessible destination, with selection, hover and focus kept distinct.
struct SettingsSidebarRow: View {
  /// The destination's label and identity.
  let item: CapsuleTab<SettingsRoot>
  /// The live subtitle for Helper, or nil for an ordinary row.
  let subtitle: String?
  /// Whether this destination owns the page shown.
  let selected: Bool
  /// Receives a selection.
  let select: @MainActor () -> Void
  /// Whether the pointer is over this row.
  @State private var hovered = false
  /// Whether the row owns keyboard focus.
  @FocusState private var focused: Bool

  /// The tile and words on the selection or hover fill.
  var body: some View {
    Button(action: select) {
      HStack(spacing: Spacing.sPlus) {
        SettingsSidebarTile(pane: item.id)
        VStack(alignment: .leading, spacing: Spacing.half) {
          Text(item.title)
            .textStyle(.body, weight: selected ? .semibold : .regular)
            .foregroundStyle(Color.textPrimary)
          if let subtitle {
            Text(subtitle)
              .textStyle(.caption)
              .foregroundStyle(Color.textSecondary)
              .swapsText(on: subtitle)
          }
        }
        .fixedSize(horizontal: false, vertical: true)
        Spacer(minLength: .zero)
      }
      .padding(.horizontal, Spacing.s)
      .padding(.vertical, Spacing.xs)
      .frame(minHeight: subtitle == nil ? SettingsLayout.sidebarRowHeight : SettingsLayout.helperSidebarRowHeight)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(
        RoundedRectangle(cornerRadius: Radius.small)
          .fill(selected ? Color.fillControlRaised : hovered ? Color.fillRowHover : .clear)
      )
      .contentShape(RoundedRectangle(cornerRadius: Radius.small))
    }
    .buttonStyle(.plain)
    .focused($focused)
    .focusEffectDisabled()
    .overlay {
      RoundedRectangle(cornerRadius: Radius.small)
        .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
        .background(
          RoundedRectangle(cornerRadius: Radius.small)
            .stroke(Color.accentRing, lineWidth: SettingsLayout.focusHalo)
        )
        .opacity(focused ? 1 : 0)
        .allowsHitTesting(false)
    }
    .onHover { hovered = $0 }
    .animation(Motion.hover(hovered), value: hovered)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(selected ? .isSelected : [])
  }
}
