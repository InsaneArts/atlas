import SwiftUI

extension View {
  /// Marks the view as one piece of a page, which fades in after the pieces before it as the page arrives (v3 S2).
  ///
  /// - Parameter piece: Which piece of the page it is.
  /// - Returns: The view, arriving in its place.
  func pagePiece(_ piece: PagePiece) -> some View {
    modifier(PageArrival(piece: piece))
  }
}
