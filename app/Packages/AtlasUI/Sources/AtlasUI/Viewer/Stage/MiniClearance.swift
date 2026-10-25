import SwiftUI

/// How far a pane's scrolling content keeps clear of the small screen in the stage's corner, at the end it sits at,
/// so the last rows can be scrolled out from under it. None while there is no small screen.
struct MiniClearance: Equatable, Sendable {
  /// The content's ends the small screen covers: the bottom, the top, or neither.
  var edges: Edge.Set = []
  /// How much room each of them keeps.
  var length: CGFloat = .zero
}
