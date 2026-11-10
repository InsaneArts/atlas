import SwiftUI

/// A page's tile in the Settings sidebar: its symbol in white on a small rounded square in its tint, lit from above as
/// an app's icon is.
struct SettingsPageTile: View {
  /// The page.
  let page: SettingsPage

  /// The tile.
  var body: some View {
    let shape = RoundedRectangle(cornerRadius: SettingsLayout.pageTileCorner, style: .continuous)
    Image(systemName: page.symbol.rawValue)
      .font(Typography.font(size: SettingsLayout.pageTileSymbol, weight: .semibold))
      .foregroundStyle(Color.monogramLetter)
      .frame(width: SettingsLayout.pageTile, height: SettingsLayout.pageTile)
      .background {
        shape
          .fill(page.tint.color)
          .overlay(shape.fill(lift))
      }
      .accessibilityHidden(true)
  }

  /// The light down the tile's top, as a monogram tile's.
  private var lift: LinearGradient {
    LinearGradient(
      stops: [
        Gradient.Stop(color: .monogramLift, location: .zero),
        Gradient.Stop(color: .clear, location: MonogramTileLayout.liftEnd),
      ],
      startPoint: .top,
      endPoint: .bottom
    )
  }
}
