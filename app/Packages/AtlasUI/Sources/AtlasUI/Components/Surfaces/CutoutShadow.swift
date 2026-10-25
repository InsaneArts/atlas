import SwiftUI

/// A shadow drawn only outside a surface's outline, as CSS draws `box-shadow`: the outline's shadow with the
/// outline itself cut away, so nothing darkens a glass surface or the words on it. The shadow is SwiftUI's
/// own, cast by an opaque copy of the outline, which the design renders draw as the window does.
struct CutoutShadow<Outline: Shape>: View {
  /// The surface's outline.
  let shape: Outline
  /// How far the surface floats.
  let level: Elevation

  /// The outline's shadow, cut away under the surface.
  var body: some View {
    shape
      // Any opaque fill: it is cut away, and only its opacity reaches the shadow.
      .fill(Color.ground)
      .elevation(level)
      .mask {
        Rectangle()
          .padding(-room)
          .overlay {
            shape
              .blendMode(.destinationOut)
          }
          .compositingGroup()
      }
      .allowsHitTesting(false)
  }

  /// How far past the surface its shadow may spread: its blur three times over below its fall, where it has faded.
  private var room: CGFloat {
    level.radius * 3 + level.y
  }
}

#if DEBUG
  #Preview("Modal") {
    let shape = RoundedRectangle(cornerRadius: Radius.extraLarge)
    shape
      .fill(Color.fillGlassThick)
      .frame(width: SampleWidth.narrow, height: SampleWidth.narrow / 2)
      .background { CutoutShadow(shape: shape, level: .modal) }
      .padding(Spacing.xxxl)
      .sampleSurface()
  }
#endif
