/// What a list asks ``CardScroll`` to scroll into view: its selected row, each time the person steps to one.
struct CardScrollReveal: Equatable, Sendable {
  /// The selected row's identity; nil while none is selected.
  var id: String?
  /// Steps made; each change scrolls the selected row into view.
  var request: Int
}
