import SwiftUI

/// The row button's label on its fill, which follows the pointer.
struct RowButtonBody: View {
  /// The button's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// Whether the Reduce Motion setting is on.
  let reduceMotion: Bool
  /// Whether the pointer is over the row.
  @State private var isHovered = false

  /// The label over the hover or pressed fill.
  var body: some View {
    configuration.label
      .contentShape(Rectangle())
      .background(
        RoundedRectangle(cornerRadius: Radius.small)
          .fill(fill)
      )
      .onHover { isHovered = $0 }
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: configuration.isPressed)
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: isHovered)
  }

  /// The fill: the pressed one while pressed, the hover one under the pointer, else none.
  private var fill: Color {
    if configuration.isPressed { return .fillControlOn }
    return isHovered ? .fillRowHover : .clear
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var presses = 0
    InteractivePreview {
      Text(verbatim: "Pressed \(presses) times")
    } content: {
      Button {
        presses += 1
      } label: {
        Text(SessionSamples.claudeName)
          .frame(maxWidth: .infinity, minHeight: PanelLayout.sessionRow, alignment: .leading)
          .padding(.horizontal, Spacing.s)
      }
      .buttonStyle(.row)
      .frame(width: PanelLayout.picture.width)
    }
  }
#endif
