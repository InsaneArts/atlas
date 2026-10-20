import SwiftUI

/// A button set like a menu item: plain text, highlighted under the pointer, with its shortcut on the right. A
/// style, so the row stays a `Button` with its own title, shortcut and disabled state. A primitive style, as it
/// draws a plain button inside, which dims a disabled title as the menu rows always have.
struct MenuRowStyle: PrimitiveButtonStyle {
  /// The shortcut written on the right, such as ⌘Q; nil writes none. The button sets it as its own shortcut too.
  var shortcut: KeyboardShortcut?

  /// Draws the title as a menu item.
  ///
  /// - Parameter configuration: The button's label and its action.
  /// - Returns: The row.
  func makeBody(configuration: Configuration) -> some View {
    MenuRowBody(configuration: configuration, detail: shortcut?.symbols)
  }
}

/// The row a menu-row button draws, which follows the pointer.
private struct MenuRowBody: View {
  /// The button's title and its action.
  let configuration: PrimitiveButtonStyleConfiguration
  /// What shows on the right; nil shows nothing.
  let detail: String?
  @State private var hovering = false
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
        RoundedRectangle(cornerRadius: Layout.radius)
          .fill(hovering && enabled ? Color.fillControl : .clear)
      )
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .opacity(enabled ? 1 : Layout.disabled)
    .onHover { hovering = $0 }
  }
}

/// The row's own measures.
private enum Layout {
  /// [R] 7: the hover's corner, kept exact until Phase 3b rounds it to `Radius.small` (8).
  static let radius: CGFloat = 7
  /// [R] 0.4: a disabled row, kept exact until Phase 3b uses `Opacity.disabled` (0.35).
  static let disabled = 0.4
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.menu)
  }
#endif
