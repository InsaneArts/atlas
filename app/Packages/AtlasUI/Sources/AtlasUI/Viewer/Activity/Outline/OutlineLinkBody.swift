import SwiftUI

/// A link as ``OutlineLinkStyle`` draws it.
struct OutlineLinkBody: View {
  /// Its words and whether it is pressed.
  let configuration: ButtonStyleConfiguration
  /// Whether a chevron follows the words.
  let chevron: Bool
  @State private var isHovered = false
  @Environment(\.isFocused) private var isFocused
  @Environment(\.hoverPaused) private var hoverPaused

  /// The words and chevron in their capsule.
  var body: some View {
    let lit = (isHovered && !hoverPaused) || configuration.isPressed
    HStack(spacing: ViewerLayout.countChevronGap) {
      configuration.label
        .font(Typography.digits(.callout))
      if chevron {
        Image(systemName: SymbolName.next.rawValue)
          .font(Typography.font(size: ViewerLayout.countChevron, weight: .semibold))
          .frame(width: ViewerLayout.countChevron, height: ViewerLayout.countChevron)
          .accessibilityHidden(true)
      }
    }
    .foregroundStyle(lit ? Color.textPrimary : Color.textSecondary)
    .lineLimit(1)
    .padding(.horizontal, ViewerLayout.linkInset)
    .frame(height: ViewerLayout.linkHeight)
    .background(
      Capsule()
        .fill(configuration.isPressed ? Color.fillControlHover : Color.fillControl)
        .opacity(lit ? 1 : 0)
        .animation(Motion.hover(isHovered), value: lit)
    )
    .overlay {
      if isFocused {
        Capsule()
          .strokeBorder(Color.strokeFocus, lineWidth: Stroke.thick)
      }
    }
    .contentShape(Capsule())
    .focusEffectDisabled()
    .fixedSize()
    .onHover { isHovered = $0 }
  }
}
