/// What leads a row's title: an original app logo, live or bundled, or a small tile for other kinds of rows.
public enum RowIcon: Equatable, Sendable {
  /// A small tile with a symbol or characters.
  case tile(TileIcon)
  /// An app's original logo, read live or bundled with Atlas.
  case app(IconImage)
  /// An agent's original mark on its brand card.
  case mark(AgentMarkKind)
}
