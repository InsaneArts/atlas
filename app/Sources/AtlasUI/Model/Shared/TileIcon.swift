/// What a small rounded tile shows: an SF Symbol, or a few monospaced characters such as a terminal's `>_`.
package enum TileIcon: Equatable, Sendable {
  /// An SF Symbol, by name.
  case symbol(String)
  /// A few monospaced characters.
  case text(String)
}
