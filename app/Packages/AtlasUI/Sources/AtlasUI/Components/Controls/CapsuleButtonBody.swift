import SwiftUI

/// The label a ``CapsuleButtonStyle`` draws: on its capsule or in its circle, filled for the pointer and the press,
/// shrinking while held unless Reduce Motion is on.
struct CapsuleButtonBody: View {
  /// The button's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// The style it is drawn in.
  let style: CapsuleButtonStyle
  @State private var isHovered = false
  @Environment(\.isEnabled) private var isEnabled

  /// The capsule or the circle, answering the pointer.
  var body: some View {
    Group {
      switch style.outline {
      case .pill: pill
      case .circle: circle
      }
    }
    .onHover { isHovered = $0 }
    .animation(Motion.hover(isHovered), value: isHovered)
  }

  /// The label on its capsule.
  private var pill: some View {
    configuration.label
      .font(Typography.font(style.textStyle, weight: style.weight))
      .tracking(style.tracking)
      .lineLimit(1)
      .padding(.horizontal, style.inset)
      .frame(maxWidth: style.fill ? .infinity : nil)
      .frame(height: style.height)
      .foregroundStyle(style.foreground)
      .background(
        Capsule()
          .fill(fill)
      )
      .background {
        if style.isGlazed {
          Color.clear
            .glass(.regular, in: Capsule())
        }
      }
      // A capsule's shape drawn as a rounded rectangle: a stroked `Capsule` leaves a hairline outside each end where
      // AppKit draws the view into an image, as the design renders do, and this one draws the same edge without it.
      .overlay(
        RoundedRectangle(cornerRadius: style.height / 2, style: .circular)
          .strokeBorder(style.edge, lineWidth: Stroke.hairline)
      )
      .contentShape(Capsule())
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.pressedScale))
      .opacity(CapsuleButtonStyle.opacity(isEnabled: isEnabled))
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
  }

  /// The symbol in its circle, a glass hairline round it.
  private var circle: some View {
    configuration.label
      .font(Typography.font(size: style.height * Layout.symbolScale, weight: .semibold))
      .foregroundStyle(style.foreground)
      .frame(width: style.height, height: style.height)
      .background(
        Circle()
          .fill(fill)
      )
      .background {
        if style.isGlazed {
          Color.clear
            .glass(.regular, in: Circle())
        }
      }
      .overlay(
        Circle()
          .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      )
      .contentShape(Circle())
      .modifier(PressScale(isPressed: configuration.isPressed, scale: Layout.circlePressedScale))
      .opacity(CapsuleButtonStyle.opacity(isEnabled: isEnabled))
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
  }

  /// The fill for the pointer and the press now.
  private var fill: Color {
    CapsuleButtonStyle.fill(
      look: style.look,
      isOn: style.isOn,
      isHovered: isHovered,
      isPressed: configuration.isPressed,
      isEnabled: isEnabled
    )
  }
}

/// The press's own measures.
private enum Layout {
  /// 0.97: how far a capsule shrinks while pressed.
  static let pressedScale: CGFloat = 0.97
  /// 0.94: how far a circle shrinks while pressed; further than a capsule, as it is smaller.
  static let circlePressedScale: CGFloat = 0.94
  /// 0.38: a circle's symbol against its side, so it sits with room around it at either size.
  static let symbolScale: CGFloat = 0.38
}
