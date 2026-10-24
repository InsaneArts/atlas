import SwiftUI

/// A glass surface behind a view, in its shape: the system glass on macOS 26 and later, the thin material under a
/// tint before it, or an opaque fill under Reduce Transparency, which it reads from the environment.
struct GlassSurface<SurfaceShape: Shape>: ViewModifier {
  /// How thick the glass is.
  let style: GlassStyle
  /// The surface's outline.
  let shape: SurfaceShape
  @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

  /// Lays the surface behind the content.
  ///
  /// - Parameter content: The view on the glass.
  /// - Returns: The view on its surface.
  func body(content: Content) -> some View {
    let backing = GlassStyle.backing(glassAvailable: Self.glassAvailable, reduceTransparency: reduceTransparency)
    switch backing {
    case .glass:
      if #available(macOS 26, *) {
        tinted(content, on: backing)
          .glassEffect(.regular, in: shape)
      } else {
        material(content)
      }
    case .material:
      material(content)
    case .opaque:
      tinted(content, on: backing)
    }
  }

  /// Whether the system draws its own glass: macOS 26 and later.
  private static var glassAvailable: Bool {
    if #available(macOS 26, *) { true } else { false }
  }

  /// The content over the thin material and its tint, as before macOS 26.
  ///
  /// - Parameter content: The view on the glass.
  /// - Returns: The view on the material.
  private func material(_ content: Content) -> some View {
    tinted(content, on: .material)
      .background(.ultraThinMaterial, in: shape)
  }

  /// The content over the style's fill for a backing, when it has one.
  ///
  /// - Parameters:
  ///   - content: The view on the glass.
  ///   - backing: What the surface is drawn with.
  /// - Returns: The view over its fill.
  private func tinted(_ content: Content, on backing: GlassStyle.Backing) -> some View {
    content
      .background {
        if let fill = style.fill(on: backing) {
          shape
            .fill(fill)
        }
      }
  }
}
