import SwiftUI

/// A picture's edge: a quiet hairline, or the blue rim and glow of control, which never tints the picture itself.
struct ControlRim: ViewModifier {
  /// Whether the person holds control.
  let isInControl: Bool
  /// The edge's corners.
  let radius: CGFloat

  /// Lays the edge over the content and the glow around it.
  ///
  /// - Parameter content: The picture.
  /// - Returns: The picture with its edge.
  func body(content: Content) -> some View {
    content
      // The glow falls from the frame's shape alone, behind the picture, so only the part outside its edge shows,
      // as the board's outer CSS glow does; out of control there is none.
      .background {
        if isInControl {
          RoundedRectangle(cornerRadius: radius)
            .fill(
              Color.accent
                .opacity(Layout.glow)
            )
            .shadow(
              color: Color.accent
                .opacity(Layout.glow),
              radius: Layout.glowRadius
            )
        }
      }
      .overlay(
        RoundedRectangle(cornerRadius: radius)
          .strokeBorder(edge, lineWidth: isInControl ? Stroke.emphasis : Stroke.hairline)
      )
  }

  /// The edge: the accent in control, a quiet hairline otherwise.
  private var edge: Color {
    guard isInControl else { return Color.separator }
    return Color.accent
      .opacity(Layout.rim)
  }
}

/// The rim's own measures, from the boards (v3.py `CONTROL_RIM`).
private enum Layout {
  /// 0.9: the accent on the rim while in control.
  static let rim = 0.9
  /// 0.3: the accent glowing around the picture while in control.
  static let glow = 0.3
  /// 7: the glow's radius, half the board's 14-point CSS blur, as a shadow's radius spreads about twice as far.
  static let glowRadius: CGFloat = 7
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.controlRim)
  }
#endif
