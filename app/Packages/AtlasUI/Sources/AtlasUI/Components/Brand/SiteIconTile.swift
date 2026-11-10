import SwiftUI

/// A site's icon on an app's tile: white in either appearance, as a web app's icon stands in the Dock, on the icon
/// grid an app's icon is drawn on, with the site's own icon centred on it at ``SiteIconTileLayout/iconShare`` of the
/// tile and a hairline edge so the white holds on a light ground.
struct SiteIconTile: View {
  /// The site's icon.
  let icon: IconImage
  /// The square's side, as a real icon's whole canvas.
  var side = IconSize.xl

  /// The tile, centred in its square.
  var body: some View {
    let body = side * IconGrid.bodyShare
    let shape = RoundedRectangle(cornerRadius: body * IconGrid.cornerShare, style: .continuous)
    shape
      .fill(Color.siteTile)
      .overlay(shape.strokeBorder(Color.siteTileEdge, lineWidth: Stroke.hairline))
      .overlay(
        Image(decorative: icon.image, scale: 1)
          .resizable()
          .interpolation(.high)
          .aspectRatio(contentMode: .fit)
          .frame(width: body * SiteIconTileLayout.iconShare, height: body * SiteIconTileLayout.iconShare)
      )
      .frame(width: body, height: body)
      .frame(width: side, height: side)
      .accessibilityHidden(true)
  }
}
