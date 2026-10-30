import SwiftUI

/// Where the small screen floats over the card's lists: the end of the list it covers and the band it takes there, its
/// height and its inset on both sides of it (``ViewerLayout/pipRoom(_:)``). None while there is no small screen.
/// ``CardScroll`` keeps that band as room at the content's end, so the list's last or first row scrolls clear of it.
struct MiniClearance: Equatable, Sendable {
  /// The list's ends the small screen covers: the bottom, the top, or neither.
  var edges: Edge.Set = []
  /// The band it takes at each of them.
  var length: CGFloat = .zero
}
