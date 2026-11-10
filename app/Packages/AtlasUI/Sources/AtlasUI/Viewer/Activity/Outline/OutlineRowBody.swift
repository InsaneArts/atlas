import SwiftUI

/// A session's or a run's row as ``OutlineRowStyle`` draws it: its highlight spans the list's width, on the filters'
/// leading edge, with the content 12 inside it on each side. No box at rest; under the pointer a soft highlight the
/// row's full height; pressed, a firmer one; with keyboard focus, a ring inside the same shape. In a box with its key
/// items, the box paints all of that for both, and the row only tells it.
struct OutlineRowBody: View {
  /// The row's label and whether it is pressed.
  let configuration: ButtonStyleConfiguration
  /// The row's height.
  let height: CGFloat
  @State private var isHovered = false
  @Environment(\.isFocused) private var isFocused
  @Environment(\.hoverPaused) private var hoverPaused
  @Environment(\.outlineBoxed) private var boxed

  /// The label at the row's height inside its highlight.
  var body: some View {
    let shape = RoundedRectangle(cornerRadius: ViewerLayout.outlineRowRadius, style: .continuous)
    configuration.label
      .frame(height: height)
      .padding(.horizontal, ViewerLayout.outlineInset)
      .background(
        // Only the paint takes the hover's fade, so the row moves as one when the list moves it.
        shape.animation(configuration.isPressed ? nil : Motion.hover(isHovered)) { highlight in
          highlight.foregroundStyle(fill)
        }
      )
      .overlay {
        if isFocused, !boxed {
          shape.strokeBorder(Color.strokeFocus, lineWidth: Stroke.thick)
        }
      }
      .contentShape(shape)
      .focusEffectDisabled()
      .onHover { isHovered = $0 }
      .preference(
        key: OutlineRowStateKey.self,
        value: OutlineRowState(isPressed: boxed && configuration.isPressed, isFocused: boxed && isFocused)
      )
  }

  /// The highlight's paint: none in a box, which paints it; else the pressed fill, the hover fill under the pointer,
  /// or none.
  private var fill: Color {
    guard !boxed else { return .clear }
    if configuration.isPressed { return .fillControl }
    return isHovered && !hoverPaused ? .fillRowHover : .clear
  }
}
