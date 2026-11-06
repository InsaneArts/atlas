import SwiftUI

/// An action large over the whole window, as a screenshot shows large: a deep shade that takes every click, closing
/// it; then on an opaque dark panel, so nothing under the shade shows through, what was done and by whom, the screen
/// before and after it to compare, its mark drawn over both, with what Atlas knows beside it, and the bar under it.
/// The shade and everything on it fade over 0.2 seconds, which Reduce Motion keeps.
struct ActionLargeLayer: View {
  /// The action shown large; nil shows none.
  let large: ViewerActionLarge?
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The shade, then the words, the pictures, the details and the bar.
  var body: some View {
    GeometryReader { proxy in
      ZStack {
        if let large {
          Color.letterbox
            .opacity(ViewerLayout.shotScrim)
            .contentShape(Rectangle())
            .onTapGesture { send(.closeAction) }
          content(large, in: proxy.size)
        }
      }
      .frame(width: proxy.size.width, height: proxy.size.height)
      .transition(.opacity)
    }
    .animation(ViewerLayout.shotFade, value: large == nil)
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(large == nil ? [] : .isModal)
  }

  /// The words over the picture and details, and the bar under them.
  ///
  /// - Parameters:
  ///   - large: The action.
  ///   - window: The window's size.
  /// - Returns: The content.
  private func content(_ large: ViewerActionLarge, in window: CGSize) -> some View {
    let size = ViewerLayout.actionPictureSize(aspect: large.aspect, in: window)
    return VStack(spacing: Spacing.mPlus) {
      HStack(alignment: .top, spacing: Spacing.sPlus) {
        if let icon = large.icon {
          SessionIcon(icon: icon, side: ViewerLayout.replayAgentIcon)
        }
        VStack(alignment: .leading, spacing: Spacing.half) {
          ActivityStepWords(words: large.words, style: .subheading, weight: .semibold)
            .environment(\.colorScheme, .dark)
          Text(verbatim: large.context)
            .font(Typography.digits(.callout))
            .foregroundStyle(Color.markBubbleDim)
        }
      }
      .lineLimit(1)
      .frame(width: size.width + Spacing.mPlus + ViewerLayout.actionDetails, alignment: .leading)
      .accessibilityElement(children: .combine)
      .accessibilityAddTraits(.isHeader)
      HStack(alignment: .top, spacing: Spacing.mPlus) {
        ActionCompare(large: large)
          .id(large.id)
          .frame(width: size.width, height: size.height)
        ActionDetailsPanel(details: large.details)
      }
      ActionLargeBar(large: large, send: send)
        .fixedSize()
    }
    .padding(ViewerLayout.actionPanelInset)
    .background(
      RoundedRectangle(cornerRadius: Radius.window, style: .continuous)
        .fill(Color.fillOverPictureHover)
    )
    .overlay(
      RoundedRectangle(cornerRadius: Radius.window, style: .continuous)
        .strokeBorder(Color.overPictureLine, lineWidth: Stroke.hairline)
    )
    .elevation(.modal)
  }
}

#if DEBUG
  #Preview("Large") {
    ActionLargeLayer(large: PreviewStates.actionLarge) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
      .windowSurface()
  }

  #Preview("Large, changes, dark") {
    var large = PreviewStates.actionLarge
    large.showsChanges = true
    return ActionLargeLayer(large: large) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
      .windowSurface()
      .environment(\.colorScheme, .dark)
  }

  #Preview("Large, nothing after") {
    ActionLargeLayer(large: PreviewStates.actionLargeFailed) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
      .windowSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerActivity
    InteractivePreview {
      Button(String("Show the typing large")) { state = PreviewStates.viewer(state, applying: .openAction("o3")) }
    } content: {
      ViewerView(state: state) { event in
        state = PreviewStates.viewer(state, applying: event)
      }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
    }
  }
#endif
