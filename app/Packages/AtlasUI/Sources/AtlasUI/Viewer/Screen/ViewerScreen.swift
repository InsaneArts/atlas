import SwiftUI

/// The Screen: the helper's screen as the only object on the stage, fitted at the picture's own shape and centred in
/// the room the card's header and footer leave, letterboxed in the card's own colour, with corners of
/// ``ViewerLayout/screenRadius`` and nothing round it but control's edge while this window holds control; or at actual
/// size in a view that scrolls. With no picture, why, on the waiting stage. A right click on the picture offers what
/// acts on it, while this window holds no control; in control the click is the helper's.
///
/// The picture itself comes from `livePicture`, the app's layer that swaps each frame whole, or an image in
/// previews and renders; the black that darkens a look is laid over it here. The shell's input surface covers
/// exactly the picture, so a click lands where the picture shows it. Fitted, the picture is the one that
/// shrinks into the corner as the Screen is left and grows back out of it (``ScreenZoom``), its corners rounding from
/// the small screen's back to its own as it grows.
struct ViewerScreen<Input: View, LivePicture: View>: View {
  /// What the screen shows.
  let state: ViewerScreenState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The surface laid over the picture that takes the mouse and keyboard.
  let input: Input
  /// Draws the newest picture.
  let livePicture: (ViewerLivePicture) -> LivePicture
  /// The namespace the picture moves in between the stage and the corner, the window's.
  let zoom: Namespace.ID
  /// The fitted picture's corners: ``ViewerLayout/screenRadius``, or the small screen's as it grows back out of it.
  var corner = ViewerLayout.screenRadius
  /// Whether the picture is moving between the stage and the corner, so its layer's corners follow the move.
  var zooms = false

  /// The picture and what lies over it, with its context menu; or why there is none.
  var body: some View {
    if let picture = state.picture {
      shown(picture)
        .contextMenu {
          CommandMenu(sections: state.menu) { send(.menu($0)) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(verbatim: "The helper's screen"))
    } else if let empty = state.empty {
      WaitingStage(empty: empty, send: send)
    }
  }

  /// The picture at actual size, or fitted, under what lies over it, with control's edge.
  ///
  /// - Parameter picture: The picture.
  /// - Returns: The screen.
  @ViewBuilder private func shown(_ picture: DesktopPicture) -> some View {
    let size = CGSize(width: picture.image.width, height: picture.image.height)
    if let actual = state.actualSize {
      ActualSizeScreen(size: actual) {
        treated(picture)
        input
        spotlight(size)
      }
      .overlay { ViewerControlEdge(isInControl: state.look == .inControl) }
    } else {
      ZStack {
        treated(picture)
        input
        spotlight(size)
      }
      .clipShape(shape)
      .overlay { ViewerControlEdge(isInControl: state.look == .inControl, corner: corner) }
      .screenZoom(in: zoom)
      .aspectRatio(size, contentMode: .fit)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
  }

  /// A window shown from the Apps list, as a spotlight over the picture and its input surface, so the label's × takes
  /// its click, fading in and out; showing another window glides it there. It is drawn dark in either appearance, as
  /// it lies on the helper's screen.
  ///
  /// - Parameter picture: The picture's size in pixels.
  /// - Returns: The spotlight, or nothing.
  private func spotlight(_ picture: CGSize) -> some View {
    ZStack {
      if let outline = state.outline {
        ScreenOutline(outline: outline, picture: picture, send: send)
          .transition(.opacity)
      }
    }
    .animation(Motion.fade, value: state.outline == nil)
    .environment(\.colorScheme, .dark)
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
        corner: corner,
        zooms: zooms
      )
    )
    .overlay(
      Color.letterbox
        .opacity(PictureTreatment.shade(for: state.look))
    )
  }

  /// The screen's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: corner, style: .continuous)
  }
}

#if DEBUG
  #Preview("You have control") {
    @Previewable @Namespace var zoom
    ViewerScreen(
      state: PreviewStates.viewerControl.screen,
      send: { _ in },
      input: EmptyView(),
      livePicture: StillPicture.init,
      zoom: zoom
    )
    .frame(width: SampleWidth.column * 2)
    .padding(Spacing.l)
    .sampleSurface()
  }

  #Preview("Logged out") {
    @Previewable @Namespace var zoom
    ViewerScreen(
      state: PreviewStates.viewerLoggedOut.screen,
      send: { _ in },
      input: EmptyView(),
      livePicture: StillPicture.init,
      zoom: zoom
    )
    .frame(width: SampleWidth.column * 2)
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    @Previewable @Namespace var zoom
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      ViewerScreen(state: state.screen, send: { _ in }, input: EmptyView(), livePicture: StillPicture.init, zoom: zoom)
        .frame(width: SampleWidth.column * 2, height: SampleWidth.column)
        .padding(Spacing.l)
    }
  }
#endif
