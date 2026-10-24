import SwiftUI

/// The helper's screen, small, over another tab: live with a blue dot, or faded with a pause glyph while the picture
/// is paused. Pressing it goes back to the screen either way. The window places it in its corner.
struct MiniScreen: View {
  /// The picture, and whether it is paused.
  let state: MiniScreenState
  /// Receives a press: back to the screen.
  let open: @MainActor () -> Void

  var body: some View {
    Button(action: open) {
      PictureFrame(picture: state.picture, look: state.isPaused ? .paused : .live, size: .small)
        .overlay(alignment: .bottomLeading) {
          FloatingChip(size: .mini) {
            chip
          }
          .padding(Spacing.s)
        }
        .frame(width: Layout.size.width, height: Layout.size.height)
        // The shadow falls from the frame's shape alone, never blurred from the picture.
        .background(
          RoundedRectangle(cornerRadius: Radius.medium)
            .stroke(Color.letterbox, lineWidth: Stroke.hairline)
            .background(
              RoundedRectangle(cornerRadius: Radius.medium)
                .fill(Color.letterbox)
                .elevation(.overlay)
            )
        )
    }
    .buttonStyle(.plain)
    .accessibilityElement()
    .accessibilityLabel("Back to the screen")
    .accessibilityValue(state.isPaused ? "Paused" : "Live")
    .accessibilityAddTraits(.isButton)
    .help("Back to the screen")
  }

  /// "Live" after a blue dot, or "Paused" after a pause glyph.
  @ViewBuilder private var chip: some View {
    if state.isPaused {
      Image(systemName: "pause.fill")
        .font(Typography.font(.caption2))
      Text("Paused")
    } else {
      StatusDot(mark: .dot, tone: .active, size: .small)
      Text("Live")
    }
  }
}

/// The mini screen's own measures, from the board (mac.py `mini`).
private enum Layout {
  /// 248 by 155: the mini screen, at the screen's 16 by 10.
  static let size = CGSize(width: 248, height: 155)
}

#if DEBUG
  #Preview("Samples") {
    MiniScreenSamples()
  }
#endif
