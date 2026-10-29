import SwiftUI

/// The label a ``CapsuleButtonStyle`` draws: on its capsule or in its circle, filled for the pointer and the press, at
/// one size whatever its state, as AppKit's buttons darken rather than shrink. No edge: the fill alone draws the
/// button. A circle is the capsule drawn its height across with no insets, so a capsule that becomes a circle, as the
/// footer's do in a narrow window, narrows into it on whatever animation carries the change. A capsule focused from the
/// keyboard draws a ring in its own shape, as a rounded rectangle whose corners are half its height, since a stroked
/// `Capsule` leaves a hairline outside each end where AppKit draws the view into an image, as the design renders do.
/// While the window is being resized the pointer's fill waits (``EnvironmentValues/hoverPaused``).
struct CapsuleButtonBody: View {
  /// The button's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// The style it is drawn in.
  let style: CapsuleButtonStyle
  @State private var isHovered = false
  @Environment(\.isEnabled) private var isEnabled
  @Environment(\.isFocused) private var isFocused
  @Environment(\.hoverPaused) private var hoverPaused

  /// The capsule or the circle, answering the pointer.
  var body: some View {
    configuration.label
      .font(isCircle ? circleFont : Typography.font(style.textStyle, weight: style.weight))
      .tracking(isCircle ? .zero : style.tracking)
      .lineLimit(isCircle ? nil : 1)
      .padding(.leading, isCircle ? .zero : style.leadingInset)
      .padding(.trailing, isCircle ? .zero : style.trailingInset)
      .frame(width: isCircle ? style.height : nil)
      .frame(maxWidth: style.fill && !isCircle ? .infinity : nil)
      .frame(height: style.height)
      .foregroundStyle(style.foreground)
      .background { surface(Capsule()) }
      .contentShape(Capsule())
      .overlay { focusRing(RoundedRectangle(cornerRadius: style.height / 2 + Layout.focusGap, style: .circular)) }
      .focusEffectDisabled()
      .opacity(CapsuleButtonStyle.opacity(isEnabled: isEnabled, isWorking: style.isWorking))
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
      .onHover { isHovered = $0 }
      .animation(Motion.hover(isLit), value: isLit)
  }

  /// Whether the label stands in a circle, a symbol alone, rather than on a capsule.
  private var isCircle: Bool {
    style.outline == .circle
  }

  /// Whether the pointer's fill shows: under the pointer, unless the window is being resized.
  private var isLit: Bool {
    isHovered && !hoverPaused
  }

  /// A circle's symbol: a glyph's at its own size and regular weight, as a title bar's symbols stand; any other
  /// look's semibold and sized from the circle's side, so it sits with room around it at either size.
  private var circleFont: Font {
    guard style.look != .glyph else { return Typography.symbol(.glyph) }
    return Typography.font(size: style.height * Layout.symbolScale, weight: .semibold)
  }

  /// The keyboard focus ring: the focus colour just outside the outline, in its shape, while the button has focus.
  ///
  /// - Parameter shape: The outline, grown to sit outside the button.
  /// - Returns: The ring, or nothing.
  @ViewBuilder private func focusRing<Ring: InsettableShape>(_ shape: Ring) -> some View {
    if isFocused {
      shape
        .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
        .padding(-Layout.focusGap)
        .allowsHitTesting(false)
    }
  }

  /// What the button is drawn on, in its shape: its fill alone.
  ///
  /// - Parameter shape: The capsule or the circle.
  /// - Returns: The surface.
  private func surface(_ shape: some Shape) -> some View {
    shape
      .fill(fill)
  }

  /// The fill for the pointer and the press now.
  private var fill: Color {
    CapsuleButtonStyle.fill(
      look: style.look,
      isOn: style.isOn,
      isHovered: isLit,
      isPressed: configuration.isPressed,
      isEnabled: isEnabled
    )
  }
}

/// The body's own measures.
private enum Layout {
  /// 2: how far outside the outline the focus ring sits, so it reads as a ring rather than an edge.
  static let focusGap = Spacing.half
  /// 0.38: a circle's symbol against its side, so it sits with room around it at either size.
  static let symbolScale: CGFloat = 0.38
}
