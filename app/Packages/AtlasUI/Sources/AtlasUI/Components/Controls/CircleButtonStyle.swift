import SwiftUI

/// A round glass button holding a symbol, beside the capsules.
struct CircleButtonStyle: ButtonStyle {
  /// How large the circle is.
  enum Size: Sendable {
    /// 34 across, a capsule's height: in a footer beside capsules.
    case regular
    /// 30 across: in a title bar and the menu bar panel.
    case small
  }

  /// How large the circle is.
  var size = Size.regular
  /// Whether what it toggles is on, which a brighter fill shows.
  var isOn = false
  @Environment(\.isEnabled) private var enabled

  /// Draws the symbol in its circle.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .font(Typography.font(size: diameter * Layout.symbolScale, weight: .semibold))
      .foregroundStyle(Color.textPrimary)
      .frame(width: diameter, height: diameter)
      .background(
        Circle()
          .fill(isOn ? Color.fillControlOn : Color.fillControl)
      )
      .overlay(
        Circle()
          .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      )
      .contentShape(Circle())
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.pressedScale))
      .opacity(enabled ? 1 : Opacity.disabled)
      .animation(Motion.press, value: configuration.isPressed)
  }

  private var diameter: CGFloat {
    switch size {
    case .regular: ControlHeight.button
    case .small: ControlHeight.row
    }
  }
}

/// The circle's own measures.
private enum Layout {
  /// 0.38: the symbol's size against the circle's, so it sits with room around it at either size.
  static let symbolScale: CGFloat = 0.38
  /// 0.94: how far a circle shrinks while pressed; further than a capsule, as it is smaller.
  static let pressedScale: CGFloat = 0.94
}

#if DEBUG
  #Preview("Samples") {
    CapsuleButtonSamples(.circles)
  }
#endif
