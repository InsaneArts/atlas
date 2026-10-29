import SwiftUI

/// A row of the sessions column as a button: an inset, rounded highlight under the row chosen, as a source list draws
/// its selection, a fainter one under the pointer and a stronger one while pressed, with no border and no scale, so
/// the row stays where it is; at once under Reduce Motion. A style, not a view, so the control stays a `Button` with
/// its own label and accessibility.
struct SidebarRowStyle: ButtonStyle {
  /// Whether the row's session's work is the one shown.
  let isSelected: Bool
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Draws the label on its highlight.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    SidebarRowBody(configuration: configuration, isSelected: isSelected, reduceMotion: reduceMotion)
  }
}
