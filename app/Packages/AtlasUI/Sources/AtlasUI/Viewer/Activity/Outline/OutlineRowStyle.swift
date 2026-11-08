import SwiftUI

/// How a session's or a run's row answers the pointer and the keyboard: no box at rest; under the pointer a soft
/// highlight the row's full height and the list's full width; pressed, a firmer one; with keyboard focus, a ring inside
/// the same shape. In a box with its key items, the box answers for both (``OutlineBox``).
struct OutlineRowStyle: ButtonStyle {
  /// The row's height.
  let height: CGFloat

  /// The row in its highlight.
  ///
  /// - Parameter configuration: The row's label and whether it is pressed.
  /// - Returns: The row.
  func makeBody(configuration: Configuration) -> some View {
    OutlineRowBody(configuration: configuration, height: height)
  }
}
