/// How the footer draws as it gives way to a narrow card, step by step: Stop and Resume as a circle or a capsule, Take
/// Control and Connecting… as their hand alone or with their words, their titles showing or faded out, and the line's
/// words. Only the card's width crossing its threshold changes it, never a press. Narrowing, the titles fade out before the
/// capsules narrow into circles; widening, the circles open before the titles fade in, so no title shows cut short.
struct FooterFold: Equatable, Sendable {
  /// Whether Stop and Resume stand as a circle round their symbol, and Take Control and Connecting… as their hand
  /// alone.
  var isCircles: Bool
  /// Whether their titles show.
  var showsTitles: Bool
  /// Whether the line keeps its words.
  var showsWords: Bool

  /// The fold settled at `fit`.
  ///
  /// - Parameter fit: How the footer gives way to the card's width.
  init(_ fit: ViewerFooterFit) {
    isCircles = fit.isCircles
    showsTitles = !fit.isCircles
    showsWords = fit.showsWords
  }
}
