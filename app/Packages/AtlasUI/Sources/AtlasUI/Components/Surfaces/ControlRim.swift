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
      // The glow falls from the frame's shape alone, behind the picture, so only the part outside its edge
      // shows, to soften the outer edge; out of control there is none.
      .background {
        if isInControl {
          RoundedRectangle(cornerRadius: radius, style: .continuous)
            .fill(
              Color.accent
                .opacity(ControlRimLayout.glow)
            )
            .shadow(
              color: Color.accent
                .opacity(ControlRimLayout.glow),
              radius: ControlRimLayout.glowRadius
            )
        }
      }
      .overlay(
        RoundedRectangle(cornerRadius: radius, style: .continuous)
          .strokeBorder(edge, lineWidth: isInControl ? Stroke.emphasis : Stroke.hairline)
      )
  }

  /// The edge: the accent in control, a quiet hairline otherwise.
  private var edge: Color {
    guard isInControl else { return Color.separator }
    return Color.accent
      .opacity(ControlRimLayout.rim)
  }
}

#if DEBUG
  #Preview("Samples") {
    ModifierSamples(.controlRim)
  }
#endif
