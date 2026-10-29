import SwiftUI

/// A sessions column row's label on its highlight, which follows the selection, the pointer and the press, inset from
/// the column's sides on corners concentric with the icon's.
struct SidebarRowBody: View {
  /// The button's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// Whether the row's session's work is the one shown.
  let isSelected: Bool
  /// Whether the Reduce Motion setting is on.
  let reduceMotion: Bool
  /// Whether the pointer is over the row.
  @State private var isHovered = false

  /// The label over its highlight, inset from the column's sides.
  var body: some View {
    configuration.label
      .padding(ViewerLayout.sidebarRowInset)
      .frame(maxWidth: .infinity, minHeight: ViewerLayout.sidebarRowHeight, alignment: .leading)
      .contentShape(shape)
      .background(shape.fill(fill))
      .padding(.horizontal, ViewerLayout.sidebarSelectionInset)
      .onHover { isHovered = $0 }
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: configuration.isPressed)
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: isHovered)
  }

  /// The highlight's shape.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: ViewerLayout.sidebarSelectionRadius, style: .continuous)
  }

  /// The highlight: the pressed fill while pressed, the selection's under the row chosen, the hover one under the
  /// pointer, else none.
  private var fill: Color {
    if configuration.isPressed { return .fillControlOn }
    if isSelected { return .fillControlRaised }
    return isHovered ? .fillRowHover : .clear
  }
}
