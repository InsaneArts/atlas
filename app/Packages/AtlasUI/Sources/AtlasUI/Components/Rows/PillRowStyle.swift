import SwiftUI

/// A row the whole width of a sidebar or a list column as a button, on a capsule: raised under the row chosen
/// (``PillSelection``), a faint fill under the pointer and a firmer one while pressed, with no border and no scale, so
/// the row stays where it is. The viewer's sessions, Settings' pages, the panel's sessions and the helper's line all
/// stand on it. It owns only the highlight and what takes the pointer; the row's label sets its own width, insets and
/// height. A style, not a view, so the control stays a `Button` with its own label and accessibility.
struct PillRowStyle: ButtonStyle {
  /// Whether the row is the one chosen.
  var isSelected = false

  /// Draws the label on its capsule.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    PillRowBody(configuration: configuration, isSelected: isSelected)
  }
}
