import SwiftUI

/// A session's or a run's row as ``OutlineRowStyle`` draws it.
struct OutlineRowBody: View {
  /// The row's label and whether it is pressed.
  let configuration: ButtonStyleConfiguration
  /// The row's height.
  let height: CGFloat
  @State private var isHovered = false
  @Environment(\.isFocused) private var isFocused
  @Environment(\.hoverPaused) private var hoverPaused

  /// The label at the row's height in its highlight, which reaches past it on each side.
  var body: some View {
    let shape = RoundedRectangle(cornerRadius: ViewerLayout.outlineRowRadius, style: .continuous)
    configuration.label
      .frame(height: height)
      .padding(.horizontal, ViewerLayout.outlineReach)
      .background(
        // Only the paint takes the hover's fade, so the row moves as one when the list moves it.
        shape.animation(configuration.isPressed ? nil : Motion.hover(isHovered)) { highlight in
          highlight.foregroundStyle(fill)
        }
      )
      .overlay {
        if isFocused {
          shape.strokeBorder(Color.strokeFocus, lineWidth: Stroke.thick)
        }
      }
      .contentShape(shape)
      .padding(.horizontal, -ViewerLayout.outlineReach)
      .focusEffectDisabled()
      .onHover { isHovered = $0 }
  }

  /// The highlight's paint: the pressed fill, the hover fill under the pointer, else none.
  private var fill: Color {
    if configuration.isPressed { return .fillControl }
    return isHovered && !hoverPaused ? .fillRowHover : .clear
  }
}
