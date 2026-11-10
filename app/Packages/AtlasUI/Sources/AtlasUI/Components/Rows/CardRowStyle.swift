import SwiftUI

/// A row inside a grouped card as a button, such as an account that opens its page or a choice: a faint fill under the
/// pointer and a firmer one while pressed, inset ``CardRowStyle/fillInset`` from the card's edges with the corner
/// nested in the card's (``Radius/nested(in:inset:)``), so the two curves run side by side. No scale, so the row stays
/// where it is. A style, not a view, so the control stays a `Button` with its own label and accessibility.
struct CardRowStyle: ButtonStyle {
  /// 4: how far the fill stands inside the card's edges.
  static let fillInset = Spacing.xs
  /// 8: the fill's corner, the card's less its inset.
  static let fillRadius = Radius.nested(in: Radius.large, inset: fillInset)

  /// Draws the label on its fill.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    CardRowBody(configuration: configuration)
  }
}
