import SwiftUI

/// A card row's label on its inset fill, which follows the pointer on ``Motion/hover(_:)`` and the press on
/// ``CapsuleButtonStyle/pressAnimation(isPressed:)``, and waits while the window is being resized
/// (``EnvironmentValues/hoverPaused``). Focused from the keyboard, it draws its own ring on the fill's inset shape, as
/// the capsule buttons draw theirs.
struct CardRowBody: View {
  /// The button's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// Whether the pointer is over the row.
  @State private var isHovered = false
  @Environment(\.hoverPaused) private var hoverPaused
  @Environment(\.isFocused) private var isFocused

  /// The label over its fill.
  var body: some View {
    configuration.label
      .contentShape(Rectangle())
      .background {
        RoundedRectangle(cornerRadius: CardRowStyle.fillRadius, style: .continuous)
          .fill(fill)
          .padding(CardRowStyle.fillInset)
      }
      .overlay { focusRing }
      .focusEffectDisabled()
      .onHover { isHovered = $0 }
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
      .animation(Motion.hover(isLit), value: isLit)
  }

  /// The keyboard focus ring: the focus colour on the fill's inset shape, while the row has focus.
  @ViewBuilder private var focusRing: some View {
    if isFocused {
      RoundedRectangle(cornerRadius: CardRowStyle.fillRadius, style: .continuous)
        .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
        .padding(CardRowStyle.fillInset)
        .allowsHitTesting(false)
    }
  }

  /// Whether the pointer's fill shows: under the pointer, unless the window is being resized.
  private var isLit: Bool {
    isHovered && !hoverPaused
  }

  /// The fill: the pressed one while pressed, the hover one under the pointer, else none.
  private var fill: Color {
    if configuration.isPressed { return .fillControlOn }
    return isLit ? .fillRowHover : .clear
  }
}
