/// What leads an action or a moment in Activity and Recent: the app it was done in, Atlas's own mark for what Atlas
/// did itself, such as recovering the helper, the person's mark for their own input, or the glyph of what it holds
/// when it was in no app. Never a bundle identifier or a path, which stay with the presenter.
public enum ActivityIcon: Equatable, Sendable {
  /// The app's icon, or the tile drawn from its name.
  ///
  /// - Parameter face: The icon or the tile.
  case app(AppIconFace)
  /// Atlas's own icon.
  case atlas
  /// The person's mark.
  case you
  /// A quiet tile with a glyph, such as a terminal's for commands.
  ///
  /// - Parameter symbol: The glyph.
  case symbol(SymbolName)
}
