import SwiftUI

/// An account's tile on its page: its app's own icon, its site's icon on a white tile, or its first letter on its
/// name's tint, as its row in the list draws it. Each body fills the square; an icon's grid margin overhangs, clear.
struct AccountTile: View {
  /// Its app's or site's icon; nil for its first letter.
  let icon: SettingsAccountIcon?
  /// Its first letter.
  let letter: String
  /// Its name, which picks the letter's tint.
  let title: String
  /// The square the tile's body fills.
  var side = IconSize.xl

  /// The tile.
  var body: some View {
    let canvas = side / IconGrid.bodyShare
    Group {
      switch icon {
      case .app(let image):
        Image(decorative: image.image, scale: 1)
          .resizable()
          .interpolation(.high)
          .frame(width: canvas, height: canvas)
      case .site(let image):
        SiteIconTile(icon: image, side: canvas)
      case nil:
        MonogramTile(monogram: AppMonogram(letter: letter, tint: MonogramTint(name: title)), side: canvas)
      }
    }
    .frame(width: side, height: side)
    .accessibilityHidden(true)
  }
}
