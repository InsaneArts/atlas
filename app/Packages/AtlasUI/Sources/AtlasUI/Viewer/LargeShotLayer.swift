import SwiftUI

/// One screenshot large over the whole window, top row included: the scrim, which takes every click, the picture at
/// its own shape between Previous and Next, and a bar with its time and size, Copy, Show in Finder and Close.
struct LargeShotLayer: View {
  /// The screenshot shown.
  let shot: ViewerLargeShot
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  var body: some View {
    GeometryReader { proxy in
      ZStack {
        Color.scrim
          .contentShape(Rectangle())
        VStack(spacing: Spacing.mPlus) {
          HStack(spacing: Spacing.mPlus) {
            Button {
              send(.previousShot)
            } label: {
              Image(systemName: "chevron.left")
            }
            .buttonStyle(.circle())
            .disabled(!shot.canGoBack)
            .help("The screenshot before")
            .accessibilityLabel("Previous")
            picture(ViewerLayout.largeShotSize(aspect: shot.aspect, in: proxy.size))
            Button {
              send(.nextShot)
            } label: {
              Image(systemName: "chevron.right")
            }
            .buttonStyle(.circle())
            .disabled(!shot.canGoForward)
            .help("The screenshot after")
            .accessibilityLabel("Next")
          }
          bar
        }
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(.isModal)
  }

  /// The picture at `size`, lifted off the scrim.
  ///
  /// - Parameter size: The picture's size.
  /// - Returns: The picture.
  private func picture(_ size: CGSize) -> some View {
    PictureTile(picture: shot.picture, radius: Radius.large)
      .frame(width: size.width, height: size.height)
      .background(
        RoundedRectangle(cornerRadius: Radius.large)
          .fill(Color.letterbox)
          .elevation(.modal)
      )
      .accessibilityElement()
      .accessibilityLabel(shot.caption)
      .accessibilityAddTraits(.isImage)
  }

  /// The bar under the picture.
  private var bar: some View {
    FloatingBar {
      Text(shot.caption)
        .font(Typography.font(.body, weight: .medium))
        .monospacedDigit()
        .padding(.horizontal, Spacing.m)
      Button("Copy") { send(.copyShot) }
        .buttonStyle(.capsule(.glass, size: .small))
        .help("Copy this screenshot as taken")
      Button("Show in Finder") { send(.showShotInFinder) }
        .buttonStyle(.capsule(.glass, size: .small))
        .disabled(!shot.canShowInFinder)
        .help("Show this screenshot's file in the Finder")
      Button("Close") { send(.closeShot) }
        .buttonStyle(.capsule(.primary, size: .small))
        .help("Back to the screenshots")
    }
  }
}

#if DEBUG
  #Preview("Large") {
    if case .screenshots(let shots) = PreviewStates.viewerShotLarge.pane, let large = shots.large {
      LargeShotLayer(shot: large) { _ in }
        .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
        .windowSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerShotLarge
    InteractivePreview {
      if case .screenshots(let shots) = state.pane, let large = shots.large {
        LargeShotLayer(shot: large) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
      }
    }
  }
#endif
