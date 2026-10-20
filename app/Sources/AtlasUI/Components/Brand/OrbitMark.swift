import SwiftUI

/// Atlas's orbit at any size, for headers: the same shape as the menu bar glyph, drawn as vectors, in the colour of
/// the text around it. `size` is the width of its box, which keeps the drawing's 54 by 32 crop; those numbers are
/// the drawing's geometry, not tokens.
struct OrbitMark: View {
  /// The width of the mark's box, in points.
  let size: CGFloat
  @Environment(\.displayScale) private var displayScale

  var body: some View {
    Canvas { context, canvas in
      let orbit = Orbit(
        center: CGPoint(x: canvas.width / 2, y: canvas.height / 2),
        width: canvas.width * 50.8 / 54,
        pixel: 1 / displayScale
      )
      // Drawn opaque as a mask, then filled once, so overlapping
      // strokes never darken in a translucent colour. The sphere has a
      // layer of its own, so the gap clears only it.
      context.clipToLayer { mask in
        mask.stroke(Path(orbit.ring), with: .color(.black), lineWidth: orbit.line)
        mask.drawLayer { sphere in
          sphere.fill(Path(orbit.sphere), with: .color(.black))
          sphere.blendMode = .destinationOut
          sphere.stroke(Path(orbit.front), with: .color(.black), lineWidth: orbit.gap)
        }
        mask.stroke(Path(orbit.front), with: .color(.black), lineWidth: orbit.line)
      }
      context.fill(Path(CGRect(origin: .zero, size: canvas)), with: .foreground)
    }
    .frame(width: size, height: size * 32 / 54)
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Orbit mark") {
    HStack(spacing: Spacing.xl) {
      OrbitMark(size: IconSize.mark)
      OrbitMark(size: IconSize.disc)
      OrbitMark(size: IconSize.avatarLarge)
    }
    .sampleSurface()
  }
#endif
