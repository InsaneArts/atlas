/// One part of a page, in the order the parts arrive as the page comes in (v3 S2, X7): the header first, then the
/// content, then the status.
enum PagePiece: Int, Sendable {
  /// The hero, the label and the title.
  case header
  /// The lede and the page's main block.
  case content
  /// The status line and the notes under it.
  case status
}
