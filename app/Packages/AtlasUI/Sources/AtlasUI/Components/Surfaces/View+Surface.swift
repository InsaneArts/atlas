import SwiftUI

extension View {
  /// Sets the view on an opaque surface in a shape.
  ///
  /// - Parameters:
  ///   - style: What the surface is.
  ///   - shape: The surface's outline, such as a capsule.
  ///   - edge: The hairline drawn just inside the outline; none by default.
  /// - Returns: The view on its surface.
  func surface(_ style: SurfaceStyle, in shape: some InsettableShape, edge: Color? = nil) -> some View {
    modifier(SolidSurface(style: style, shape: shape, edge: edge))
  }
}

#if DEBUG
  #Preview("Surfaces") {
    VStack(spacing: Spacing.l) {
      ForEach(SurfaceStyle.allCases, id: \.self) { style in
        Text(verbatim: "\(style)")
          .textStyle(.body)
          .padding(.horizontal, Spacing.l)
          .frame(height: ControlHeight.sPlus)
          .surface(style, in: Capsule(), edge: .strokeStrong)
      }
    }
    .padding(Spacing.xxxl)
    .sampleSurface()
  }
#endif
