import SwiftUI

/// The helper's screen, small, over a list: the live picture alone, faded while it is paused, on a shadow that deepens
/// under the pointer. Pressing it goes back to the screen either way. It fills the room it is given, so the window
/// sizes it in its corner, and the same frame can grow from the stage's picture and back, its corners rounding between
/// the stage's and its own as it goes.
///
/// The picture is the window's own: the same live slot the stage draws, fitted at its own shape in the colour
/// its look keeps; the black that darkens a paused picture is laid over it here.
struct MiniScreen<Picture: View>: View {
  /// The picture, and whether it is paused.
  let state: MiniScreenState
  /// The corners' radius: its own, or the stage's while it grows out of the stage's picture.
  let corner: CGFloat
  /// Receives a press: back to the screen.
  let open: @MainActor () -> Void
  /// Draws the picture, fitted.
  let picture: Picture
  /// Whether the pointer is over it.
  @State private var isHovered = false
  @Environment(\.hoverPaused) private var hoverPaused

  /// Creates the small screen.
  ///
  /// - Parameters:
  ///   - state: The picture, and whether it is paused.
  ///   - corner: The corners' radius.
  ///   - open: Receives a press: back to the screen.
  ///   - picture: Draws the picture, fitted, in the colour its look keeps, cut to `corner`.
  init(
    state: MiniScreenState,
    corner: CGFloat,
    open: @escaping @MainActor () -> Void,
    @ViewBuilder picture: () -> Picture
  ) {
    self.state = state
    self.corner = corner
    self.open = open
    self.picture = picture()
  }

  /// The picture in its corners, over a shadow from its shape.
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
      // The shadow falls from the frame's shape alone, never blurred from the picture.
      .background(
        shape
          .fill(Color.letterbox)
          .elevation(isHovered && !hoverPaused ? .overlay : .floating)
      )
    }
    .buttonStyle(.plain)
    .onHover { hovering in
      withAnimation(Motion.hover(hovering)) { isHovered = hovering }
    }
    .accessibilityElement()
    .accessibilityLabel("Back to the screen")
    .accessibilityValue(state.isPaused ? "Paused" : "Live")
    .accessibilityAddTraits(.isButton)
    .help("Back to the screen")
  }

  /// The small screen's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: corner, style: .continuous)
  }
}

#if DEBUG
  #Preview("Samples") {
    MiniScreenSamples()
  }
#endif
