import SwiftUI

/// A sidebar row as a button: a raised capsule under the row chosen, a fainter one under the pointer and a stronger
/// one while pressed, with no border and no scale, so the row stays where it is. A style, not a view, so the control
/// stays a `Button` with its own label and accessibility.
struct SidebarRowStyle: ButtonStyle {
  /// Whether the row is the one chosen.
  let isSelected: Bool
  /// The row's least height.
  let height: CGFloat

  /// Draws the label on its highlight.
  ///
  /// - Parameter configuration: The button's label and pressed state.
  /// - Returns: The styled button.
  func makeBody(configuration: Configuration) -> some View {
    SidebarRowBody(
      configuration: configuration,
      isSelected: isSelected,
      height: height
    )
  }
}
