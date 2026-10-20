import SwiftUI

/// The row a menu-row button draws, which follows the pointer.
struct MenuRowBody: View {
  /// The button's title and its action.
  let configuration: PrimitiveButtonStyleConfiguration
  /// What shows on the right; nil shows nothing.
  let detail: String?
  /// Whether the pointer is over the row.
  @State private var hovering = false
  /// Whether the button can be pressed.
  @Environment(\.isEnabled) private var enabled

  var body: some View {
    Button(action: configuration.trigger) {
      HStack {
        configuration.label
          .font(Typography.font(.body))
        Spacer()
        if let detail {
          Text(detail)
            .font(Typography.font(.body))
            .foregroundStyle(Color.textTertiary)
        }
      }
      .padding(.horizontal, Spacing.m)
      .frame(height: ControlHeight.menuRow)
      .background(
        RoundedRectangle(cornerRadius: Radius.small)
          .fill(hovering && enabled ? Color.fillControl : .clear)
      )
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .opacity(enabled ? 1 : Opacity.disabled)
    .onHover { hovering = $0 }
  }
}
