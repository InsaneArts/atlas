import CoreGraphics

/// How the footer gives way as the card narrows: its capsules with their words; then Stop and Take Control as circles
/// round their symbols, the latest line cut short beside them; then, narrower still, the line down to its session's icon
/// and its age.
public enum ViewerFooterFit: Equatable, Sendable {
  /// Every capsule with its words, and the whole line.
  case full
  /// Stop, Resume and Take Control as circles round their symbols; the line cut short.
  case compact
  /// The circles, and the line down to its icon and its age.
  case tight

  /// Whether Stop, Resume and Take Control stand as circles round their symbols.
  var isCircles: Bool { self != .full }

  /// Whether the line keeps its words.
  var showsWords: Bool { self != .tight }

  /// How the footer stands in a card `cardWidth` wide, from how it stands now: its words go under the card width
  /// ``ViewerLayout/footerWords`` gives way at and come back only past it, and the line's words likewise at
  /// ``ViewerLayout/footerLine``.
  ///
  /// - Parameters:
  ///   - cardWidth: The card's width.
  ///   - current: How the footer stands now.
  /// - Returns: How it stands at that width.
  public static func fitting(cardWidth: CGFloat, current: Self) -> Self {
    if ViewerLayout.footerWords.isAbove(cardWidth, wasAbove: current == .full) { return .full }
    if ViewerLayout.footerLine.isAbove(cardWidth, wasAbove: current != .tight) { return .compact }
    return .tight
  }
}
