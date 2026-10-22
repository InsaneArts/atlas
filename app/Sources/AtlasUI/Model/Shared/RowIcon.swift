/// What leads a row's title as its icon: an app's own icon where the app is on this Mac, else a small tile.
package enum RowIcon: Equatable, Sendable {
  /// A small tile with a symbol or characters.
  case tile(TileIcon)
  /// The app's own icon.
  case app(IconImage)
}
