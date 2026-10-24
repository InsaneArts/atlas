import SwiftUI

/// The helper's screen, small, over another tab: live with a blue dot, or faded with a pause glyph while the picture
/// is paused. Pressing it goes back to the screen either way. It fills the room it is given, so the window sizes it in
/// its corner, and the same frame can grow from the stage's picture and back (v3 V16).
///
/// The picture is the window's own: the same live slot the stage draws (v3 V17), fitted at its own shape in the
/// colour its look keeps; the black that darkens a paused picture is laid over it here.
struct MiniScreen<Picture: View>: View {
  /// The picture, and whether it is paused.
  let state: MiniScreenState
  /// Receives a press: back to the screen.
  let open: @MainActor () -> Void
  /// Draws the picture, fitted.
  let picture: Picture

  /// Creates the small screen.
  ///
  /// - Parameters:
  ///   - state: The picture, and whether it is paused.
  ///   - open: Receives a press: back to the screen.
  ///   - picture: Draws the picture, fitted, in the colour its look keeps.
  init(state: MiniScreenState, open: @escaping @MainActor () -> Void, @ViewBuilder picture: () -> Picture) {
    self.state = state
    self.open = open
    self.picture = picture()
  }

  /// The picture on black with its hairline, Live or Paused in its corner, over a shadow from its shape.
  var body: some View {
    Button(action: open) {
      ZStack {
        Color.letterbox
        picture
          .overlay(
            Color.letterbox
              .opacity(PictureTreatment.shade(for: state.look))
          )
      }
      .clipShape(shape)
      .controlRim(false, radius: Radius.medium)
      .overlay(alignment: .bottomLeading) {
        FloatingChip(size: .mini) {
          chip
        }
        .padding(Spacing.s)
      }
      // The shadow falls from the frame's shape alone, never blurred from the picture.
      .background(
        shape
          .stroke(Color.letterbox, lineWidth: Stroke.hairline)
          .background(
            shape
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

  /// The small screen's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: Radius.medium)
  }
}

#if DEBUG
  #Preview("Samples") {
    MiniScreenSamples()
  }
#endif
