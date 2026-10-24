import SwiftUI

/// The Screen tab: the helper's screen as its own card, fitted at the picture's shape inside the frame every tab
/// shares (the frame is only its largest size) and centred there, the window's ground around it; or at actual size
/// in a view that scrolls; with what is happening laid over the picture. With no picture, why, on the whole frame.
/// The shell's input surface covers exactly the picture, so a click lands where the picture shows it.
struct ViewerScreen<Input: View>: View {
  /// What the screen shows.
  let state: ViewerScreenState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The surface laid over the picture that takes the mouse and keyboard.
  let input: Input

  var body: some View {
    if let picture = state.picture {
      shown(picture)
        // The shadow falls from the screen's shape alone, never blurred from the picture.
        .background {
          RoundedRectangle(cornerRadius: Radius.large)
            .fill(Color.letterbox)
            .elevation(.raised)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("The helper's screen")
    } else if let empty = state.empty {
      EmptyMessage(state: empty.message) {
        if let event = empty.event { send(event) }
      }
    }
  }

  /// The picture at actual size, or fitted, under what lies over it.
  ///
  /// - Parameter picture: The picture.
  /// - Returns: The screen.
  @ViewBuilder private func shown(_ picture: DesktopPicture) -> some View {
    if let size = state.actualSize {
      ActualSizeScreen(picture: picture, size: size, look: state.look, outline: state.outline, input: input)
        .overlay { ScreenOverlay(state: state) }
    } else {
      ZStack {
        PictureFrame(picture: picture, look: state.look)
        if let outline = state.outline {
          ScreenOutline(outline: outline, picture: CGSize(width: picture.image.width, height: picture.image.height))
        }
        input
        ScreenOverlay(state: state)
      }
      .aspectRatio(CGSize(width: picture.image.width, height: picture.image.height), contentMode: .fit)
    }
  }
}

#if DEBUG
  #Preview("You have control") {
    ViewerScreen(state: PreviewStates.viewerControl.screen, send: { _ in }, input: EmptyView())
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
  }

  #Preview("Logged out") {
    ViewerScreen(state: PreviewStates.viewerLoggedOut.screen, send: { _ in }, input: EmptyView())
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
  }
#endif
