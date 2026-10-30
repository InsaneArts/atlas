import SwiftUI

/// An app's icon at a square side, or the tile drawn from its name where none was found, so an app never shows as a
/// blank square. When its own icon is found the tile crossfades into it in place.
struct AppGlyph: View {
  /// The icon or the tile.
  let face: AppIconFace
  /// The square's side, as a real icon's whole canvas.
  var side = IconSize.xl

  /// The icon or the tile at its side.
  var body: some View {
    ZStack {
      switch face {
      case .image(let icon):
        Image(decorative: icon.image, scale: 1)
          .resizable()
          .interpolation(.high)
          .id(icon.id)
      case .monogram(let monogram):
        MonogramTile(monogram: monogram, side: side)
      }
    }
    .frame(width: side, height: side)
    .animation(Motion.crossfade, value: face)
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Samples") {
    AppGlyphSamples(.beside)
  }
#endif
