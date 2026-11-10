import AtlasStatus

/// What Atlas's menu bar item shows: its badge's row and the word its tooltip and VoiceOver read.
struct PanelGlyph: Equatable {
  /// The row, whose badge the glyph draws.
  var kind: Mode.Kind
  /// The word, such as "Working".
  var word: String
}
