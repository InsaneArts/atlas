import SwiftUI

/// A sidebar row's label on its capsule: raised and white with a fine rim under the row chosen
/// (``SidebarSelectionPill``), which moves to another row at once as the choice changes, nothing travelling between
/// them; a faint fill under the pointer and a firmer one while pressed, which gives way to the selection at once.
struct SidebarRowBody: View {
  /// The button's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// Whether the row is the one chosen.
  let isSelected: Bool
  /// The row's least height.
  let height: CGFloat
  /// Whether the pointer is over the row.
  @State private var isHovered = false
  @Environment(\.hoverPaused) private var hoverPaused

  /// The label over its capsule.
  var body: some View {
    configuration.label
      .frame(width: ViewerLayout.rowContentWidth, alignment: .leading)
      .padding(.horizontal, ViewerLayout.sidebarRowInset)
      .frame(minHeight: height)
      .contentShape(Capsule())
      .background { highlight }
      .onHover { isHovered = $0 }
      .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
      .animation(Motion.hover(isLit), value: isLit)
  }

  /// The highlight: the selection under the row chosen, else the pressed or the hover fill, or none; a change of the
  /// row chosen lands at once, with no animation, so the selection never fades, travels or shows over a fill.
  private var highlight: some View {
    ZStack {
      if isSelected {
        SidebarSelectionPill()
      } else {
        Capsule()
          .fill(fill)
      }
    }
    .transaction(value: isSelected) { $0.animation = nil }
  }

  /// A row's fill while not chosen: the pressed one while pressed, the hover one under the pointer, else none.
  private var fill: Color {
    if configuration.isPressed { return .fillControlOn }
    return isLit ? .fillRowHover : .clear
  }

  /// Whether the pointer's fill shows: under the pointer, unless the window is being resized.
  private var isLit: Bool {
    isHovered && !hoverPaused
  }
}
