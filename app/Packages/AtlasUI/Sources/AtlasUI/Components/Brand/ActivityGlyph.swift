import SwiftUI

/// What leads an action or a moment, at a square side: the app's icon or its tile, Atlas's own icon, the person's
/// blue mark, or a quiet tile with a glyph, all on the icon grid so they line up in one column at one size. An icon
/// found later crossfades in place.
struct ActivityGlyph: View {
  /// What it draws.
  let icon: ActivityIcon
  /// The square's side, as a real icon's whole canvas.
  var side = IconSize.mPlus

  /// The icon at its side.
  var body: some View {
    Group {
      switch icon {
      case .app(let face): AppGlyph(face: face, side: side)
      case .atlas: AppIcon(canvas: side)
      case .you: IconGridTile(symbol: .person, tone: .accent, side: side)
      case .symbol(let symbol): IconGridTile(symbol: symbol, side: side)
      }
    }
    .frame(width: side, height: side)
    .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Samples") {
    AppGlyphSamples(.activity)
  }
#endif
