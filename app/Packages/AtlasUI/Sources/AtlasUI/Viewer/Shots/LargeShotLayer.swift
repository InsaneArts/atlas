import SwiftUI

/// A screenshot large over the whole window, top row included (v3 V10, `.vlarge`): a deep shade that takes every
/// click, closing it, and fades over 0.22 seconds, the picture zoomed from its tile to the centre between Previous
/// and Next, and one glass bar at the window's foot with its time and size, Copy, Show in Finder and Close. Closing
/// zooms the picture back to its tile. Under Reduce Motion the picture fades in and out in place.
struct LargeShotLayer: View {
  /// The screenshot shown large; nil shows none.
  let shot: ViewerLargeShot?
  /// The namespace the picture zooms in from its tile, the window's.
  let zoom: Namespace.ID
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The shade, the picture and its buttons, over the window.
  var body: some View {
    GeometryReader { proxy in
      ZStack {
        scrim
        picture(in: proxy.size)
        chrome(in: proxy.size)
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(shot == nil ? [] : .isModal)
  }

  /// The shade over the window, which takes every click while a screenshot shows; a click on it closes the screenshot.
  private var scrim: some View {
    ZStack {
      if shot != nil {
        Color.letterbox
          .opacity(ViewerLayout.shotScrim)
          .contentShape(Rectangle())
          .onTapGesture { send(.closeShot) }
          .transition(.opacity)
      }
    }
    .animation(ViewerLayout.shotFading(reduceMotion: reduceMotion), value: shot == nil)
  }

  /// The picture at its own shape in the centre, zooming from and back to its tile.
  ///
  /// - Parameter window: The window's size.
  /// - Returns: The picture, or nothing.
  private func picture(in window: CGSize) -> some View {
    ZStack {
      if let shot {
        let size = ViewerLayout.largeShotSize(aspect: shot.aspect, in: window)
        // The picture and its backing carry the zoom inside the fixed frame, so the frame the zoom hands them really
        // resizes them from the tile's to the centre's, rather than moving a picture already full size.
        PictureTile(picture: shot.picture, radius: Radius.large)
          .background(
            RoundedRectangle(cornerRadius: Radius.large)
              .filledOutline(Color.letterbox, border: Color.strokeStrong)
              .elevation(.modal)
          )
          .shotZoom(id: shot.id, in: zoom)
          .frame(width: size.width, height: size.height)
          .accessibilityElement()
          .accessibilityLabel(shot.caption)
          .accessibilityAddTraits(.isImage)
          .transition(.opacity)
      }
    }
    .animation(Motion.animation(Motion.zoom, reduceMotion: reduceMotion, reduced: Motion.reduced), value: shot == nil)
  }

  /// Previous and Next beside the picture, and the bar at the window's foot, fading in and out with the shade.
  ///
  /// - Parameter window: The window's size.
  /// - Returns: The buttons and the bar, or nothing.
  private func chrome(in window: CGSize) -> some View {
    ZStack {
      if let shot {
        let size = ViewerLayout.largeShotSize(aspect: shot.aspect, in: window)
        HStack(spacing: Spacing.mPlus) {
          step(.back, label: "Previous", help: "The screenshot before", enabled: shot.canGoBack) {
            send(.previousShot)
          }
          Color.clear
            .frame(width: size.width, height: size.height)
            .accessibilityHidden(true)
          step(.next, label: "Next", help: "The screenshot after", enabled: shot.canGoForward) {
            send(.nextShot)
          }
        }
        .transition(.opacity)
        LargeShotBar(shot: shot, send: send)
          .fixedSize()
          .frame(maxHeight: .infinity, alignment: .bottom)
          .padding(.bottom, ViewerLayout.shotBarInset)
          .transition(.opacity)
      }
    }
    .animation(ViewerLayout.shotFading(reduceMotion: reduceMotion), value: shot == nil)
  }

  /// A round glass button stepping to the screenshot before or after.
  ///
  /// - Parameters:
  ///   - symbol: Its chevron.
  ///   - label: What VoiceOver calls it.
  ///   - help: Its tooltip.
  ///   - enabled: Whether there is a screenshot that way.
  ///   - action: What it does.
  /// - Returns: The button.
  private func step(
    _ symbol: SymbolName,
    label: String,
    help: String,
    enabled: Bool,
    action: @escaping @MainActor () -> Void
  ) -> some View {
    Button(action: action) {
      Image(systemName: symbol.rawValue)
    }
    .buttonStyle(.capsule(.glass, size: .regular, outline: .circle))
    .disabled(!enabled)
    .help(help)
    .accessibilityLabel(label)
  }
}

#if DEBUG
  #Preview("Large") {
    @Previewable @Namespace var zoom
    if case .screenshots(let shots) = PreviewStates.viewerShotLarge.pane {
      LargeShotLayer(shot: shots.large, zoom: zoom) { _ in }
        .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
        .windowSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerShotLarge
    @Previewable @Namespace var zoom
    InteractivePreview {
      if case .screenshots(let shots) = state.pane {
        LargeShotLayer(shot: shots.large, zoom: zoom) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
      }
    }
  }
#endif
