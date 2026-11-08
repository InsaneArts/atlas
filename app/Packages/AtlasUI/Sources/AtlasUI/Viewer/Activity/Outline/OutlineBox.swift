import SwiftUI

/// A folded row and its key items, its problem lines or its newest pictures, as one box: the pointer over any of it
/// lights the whole box, pressing its row presses the whole box, and keyboard focus on its row rings the whole box. It
/// spans the list's width, on the row's own edges, with 12 under its last item. Unboxed, it adds no box, so a row keeps
/// its place, its focus and its chevron's turn as it opens out of its box or folds into one. Boxed or not, it stands
/// on the card's ground (``SwiftUI/View/outlineGround()``).
struct OutlineBox: ViewModifier {
  /// Whether the row and its key items stand as one box.
  let isBoxed: Bool
  @State private var isHovered = false
  @State private var row = OutlineRowState()
  @Environment(\.hoverPaused) private var hoverPaused

  /// The row and its items, boxed or as they are.
  func body(content: Content) -> some View {
    let shape = RoundedRectangle(cornerRadius: ViewerLayout.outlineRowRadius, style: .continuous)
    content
      .padding(.bottom, isBoxed ? ViewerLayout.boxBelow : .zero)
      .frame(maxWidth: .infinity, alignment: .leading)
      .environment(\.outlineBoxed, isBoxed)
      .background(
        // Only the paint takes the hover's fade, so the box moves as one when the list moves it.
        shape.animation(row.isPressed ? nil : Motion.hover(isHovered)) { highlight in
          highlight.foregroundStyle(fill)
        }
      )
      .outlineGround()
      .overlay {
        if isBoxed, row.isFocused {
          shape.strokeBorder(Color.strokeFocus, lineWidth: Stroke.thick)
        }
      }
      .onHover { isHovered = $0 }
      .onPreferenceChange(OutlineRowStateKey.self) { row = $0 }
  }

  /// The box's paint: none unboxed; else the pressed fill while its row is pressed, the hover fill under the pointer,
  /// or none.
  private var fill: Color {
    guard isBoxed else { return .clear }
    if row.isPressed { return .fillControl }
    return isHovered && !hoverPaused ? .fillRowHover : .clear
  }
}
