import SwiftUI

/// The row button's label on its fill, which follows the pointer, and waits while the window is being resized
/// (``EnvironmentValues/hoverPaused``).
struct RowButtonBody: View {
  /// The button's label and pressed state.
  let configuration: ButtonStyleConfiguration
  /// The fill's corner.
  let corner: CGFloat
  /// Whether the Reduce Motion setting is on.
  let reduceMotion: Bool
  /// Whether the pointer is over the row.
  @State private var isHovered = false
  @Environment(\.hoverPaused) private var hoverPaused

  /// The label over the hover or pressed fill.
  var body: some View {
    configuration.label
      .contentShape(Rectangle())
      .background(
        RoundedRectangle(cornerRadius: corner, style: .continuous)
          .fill(fill)
      )
      .onHover { isHovered = $0 }
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: configuration.isPressed)
      .animation(Motion.animation(Motion.press, reduceMotion: reduceMotion), value: isLit)
  }

  /// Whether the pointer's fill shows: under the pointer, unless the window is being resized.
  private var isLit: Bool {
    isHovered && !hoverPaused
  }

  /// The fill: the pressed one while pressed, the hover one under the pointer, else none.
  private var fill: Color {
    if configuration.isPressed { return .fillControlOn }
    return isLit ? .fillRowHover : .clear
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
