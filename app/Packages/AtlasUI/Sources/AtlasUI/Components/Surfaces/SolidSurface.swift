import SwiftUI

/// An opaque surface behind a view, in its shape: the style's fill, never glass or a material, and a hairline edge
/// over it when one is asked for.
struct SolidSurface<SurfaceShape: InsettableShape>: ViewModifier {
  /// What the surface is.
  let style: SurfaceStyle
  /// The surface's outline.
  let shape: SurfaceShape
  /// The hairline drawn just inside the outline, or nil for none.
  let edge: Color?

  /// Lays the fill behind the content and the edge over it.
  ///
  /// - Parameter content: The view on the surface.
  /// - Returns: The view on its surface.
  func body(content: Content) -> some View {
    content
      .background {
        shape
          .fill(style.fill)
      }
      .overlay {
        if let edge {
          shape
            .strokeBorder(edge, lineWidth: Stroke.hairline)
            .allowsHitTesting(false)
        }
      }
  }
}
