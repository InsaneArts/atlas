import SwiftUI

extension View {
  /// Stands an outline row on the card's ground, reaching ``ViewerLayout/outlineCurtain`` below it under the rows that
  /// follow, as a curtain: what a disclosure above brings in, laid out in its place at once, stays covered by the rows
  /// below until they move down past it and is uncovered as they do; what a fold takes away is covered as they move
  /// back up. At rest the rows that follow stand over it, so it never shows.
  ///
  /// - Returns: The row on its ground.
  func outlineGround() -> some View {
    background(alignment: .top) {
      Color.fillContent
        .padding(.bottom, -ViewerLayout.outlineCurtain)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
  }
}
