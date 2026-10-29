import SwiftUI

/// The whole sidebar shown over the card, as the toggle or Find shows it while the window is too narrow for it: on the
/// solid recessed ground, raised off the card by its shadow, its search's field taking the focus as it appears. It
/// slides in from the window's leading edge and back out on ``Motion/slide`` (``SidebarSplit`` moves it), and goes with
/// a choice, a click on the card, Escape or the toggle.
struct SidebarOverlay: View {
  /// What the sidebar shows.
  let sidebar: ViewerSidebarState
  /// Whether the window is in full screen, which hides the window buttons the top bar makes room for.
  let isFullScreen: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The sidebar, whole, on its raised ground.
  var body: some View {
    ViewerSidebar(sidebar: sidebar, isFullScreen: isFullScreen, send: send)
      .frame(width: ViewerLayout.sidebarWidth)
      .frame(maxHeight: .infinity)
      .background {
        Rectangle()
          .fill(Color.groundRecessed)
          .elevation(.raised)
      }
  }
}

#if DEBUG
  #Preview("Over the card") {
    SidebarOverlay(sidebar: PreviewStates.viewerSidebarOverlay.sidebar, isFullScreen: false) { _ in }
      .frame(maxWidth: .infinity, alignment: .leading)
      .frame(width: SampleWidth.narrow * 2, height: SampleWidth.narrow * 2)
      .background(Color.fillContent)
  }
#endif
