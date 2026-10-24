import CoreGraphics
import SwiftUI

/// An app's icon at a square size, or a quiet tile with the generic app glyph where the app has none to show.
struct AppTile: View {
  /// The icon's pixels; nil draws the tile.
  let image: CGImage?
  /// The icon's side: an app's 34 by default, 16 beside a label.
  var side = IconSize.xl

  /// The icon, or the quiet tile, at its side.
  var body: some View {
    icon
      .frame(width: side, height: side)
      .accessibilityHidden(true)
  }

  /// The icon, or the tile.
  @ViewBuilder private var icon: some View {
    if let image {
      Image(decorative: image, scale: 1)
        .resizable()
        .interpolation(.high)
    } else {
      RoundedRectangle(cornerRadius: side * Layout.cornerShare, style: .continuous)
        .fill(Color.fillControl)
        .overlay(
          RoundedRectangle(cornerRadius: side * Layout.cornerShare, style: .continuous)
            .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
        )
        .overlay(
          Image(systemName: "app")
            .resizable()
            .scaledToFit()
            .foregroundStyle(Color.textTertiary)
            .padding(side * Layout.glyphInset)
        )
    }
  }
}

/// The tile's own measures.
private enum Layout {
  /// 0.225: the tile's corners against its side, as an app icon's are drawn on the Mac.
  static let cornerShare: CGFloat = 0.225
  /// 0.25: the generic glyph's inset against the tile's side, so it sits inside as an icon's artwork does.
  static let glyphInset: CGFloat = 0.25
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.m) {
      AppTile(image: nil)
      AppTile(image: nil, side: IconSize.s)
    }
    .sampleSurface()
  }
#endif
