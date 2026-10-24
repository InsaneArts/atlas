import SwiftUI

/// The Screen tab (v3 V4): the helper's screen as the only object on the stage, fitted at the picture's own shape
/// inside the frame every tab shares and centred there, with 12-point corners, a fine light inner edge and a short
/// shadow on the window's ground; or at actual size in a view that scrolls; with what is happening laid over the
/// picture and control's rim round it (v3 V5). With no picture, why, on the whole frame.
///
/// The picture itself comes from `livePicture`, the app's layer that swaps each frame whole (v3 V17), or an image in
/// previews and renders; the black that darkens a look is laid over it here. The shell's input surface covers exactly
/// the picture, so a click lands where the picture shows it.
struct ViewerScreen<Input: View, LivePicture: View>: View {
  /// What the screen shows.
  let state: ViewerScreenState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The surface laid over the picture that takes the mouse and keyboard.
  let input: Input
  /// Draws the newest picture.
  let livePicture: (ViewerLivePicture) -> LivePicture

  /// The picture under what lies over it, or why there is none.
  var body: some View {
    if let picture = state.picture {
      shown(picture)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(verbatim: "The helper's screen"))
    } else if let empty = state.empty {
      EmptyMessage(state: empty.message) {
        if let event = empty.event { send(event) }
      }
    }
  }

  /// The picture at actual size, or fitted, under what lies over it, inside control's rim.
  ///
  /// - Parameter picture: The picture.
  /// - Returns: The screen.
  @ViewBuilder private func shown(_ picture: DesktopPicture) -> some View {
    let size = CGSize(width: picture.image.width, height: picture.image.height)
    if let actual = state.actualSize {
      ActualSizeScreen(size: actual) {
        treated(picture)
        if let outline = state.outline {
          ScreenOutline(outline: outline, picture: size)
        }
        input
      }
      .overlay { ScreenOverlay(state: state) }
      .overlay { ViewerControlRim(isInControl: state.look == .inControl) }
    } else {
      ZStack {
        Color.letterbox
        treated(picture)
        if let outline = state.outline {
          ScreenOutline(outline: outline, picture: size)
        }
        input
        ScreenOverlay(state: state)
      }
      .clipShape(shape)
      .overlay(
        shape
          .strokeBorder(Color.strokeHighlight, lineWidth: Stroke.hairline)
      )
      // The shadow falls from the screen's shape alone, never blurred from the picture.
      .background {
        shape
          .fill(Color.letterbox)
          .elevation(.resting)
      }
      .overlay { ViewerControlRim(isInControl: state.look == .inControl) }
      .aspectRatio(size, contentMode: .fit)
    }
  }

  /// The picture through the live slot, with the colour its look keeps and the black that darkens it.
  ///
  /// - Parameter picture: The picture.
  /// - Returns: The picture, treated.
  private func treated(_ picture: DesktopPicture) -> some View {
    livePicture(
      ViewerLivePicture(
        picture: picture,
        saturation: PictureTreatment.saturation(for: state.look),
        corner: Radius.large
      )
    )
    .overlay(
      Color.letterbox
        .opacity(PictureTreatment.shade(for: state.look))
    )
  }

  /// The screen's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: Radius.large)
  }
}

#if DEBUG
  #Preview("You have control") {
    ViewerScreen(
      state: PreviewStates.viewerControl.screen,
      send: { _ in },
      input: EmptyView(),
      livePicture: StillPicture.init
    )
    .frame(width: SampleWidth.column * 2)
    .padding(Spacing.l)
    .sampleSurface()
  }

  #Preview("Logged out") {
    ViewerScreen(
      state: PreviewStates.viewerLoggedOut.screen,
      send: { _ in },
      input: EmptyView(),
      livePicture: StillPicture.init
    )
    .frame(width: SampleWidth.column * 2)
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      ViewerScreen(state: state.screen, send: { _ in }, input: EmptyView(), livePicture: StillPicture.init)
        .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
        .padding(Spacing.l)
    }
  }
#endif
