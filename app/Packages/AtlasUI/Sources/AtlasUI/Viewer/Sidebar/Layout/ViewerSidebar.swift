import SwiftUI

/// The sidebar's content, straight on the window's recessed ground as a source list stands, with no border: the top
/// bar with the search, All sessions and each section's sessions, then the helper's health at its foot, which stays
/// put. The sessions scroll in their own room, which ends a section's gap above the foot: a row is cut at that edge,
/// never drawn under the foot, and fades out over its last points into the ground, so a cut row reads as scrolled rather
/// than broken; scrolled to its end, the last row stands clear of the fade. In a short window the foot folds into the
/// helper's dot alone, fading between the two on ``Motion/state``.
struct ViewerSidebar: View {
  /// What the sidebar shows.
  private let sidebar: ViewerSidebarState
  /// Whether the window is in full screen, which hides the window buttons the top bar makes room for.
  private let isFullScreen: Bool
  /// Receives what the person does.
  private let send: @MainActor (ViewerEvent) -> Void
  /// Whether the foot is drawn folded for a short window, which follows the state's on ``Motion/state``.
  @State private var isShort: Bool
  @Environment(\.motionReduced) private var reduceMotion

  /// Creates the sidebar's content.
  ///
  /// - Parameters:
  ///   - sidebar: What the sidebar shows.
  ///   - isFullScreen: Whether the window is in full screen.
  ///   - send: Receives what the person does.
  init(
    sidebar: ViewerSidebarState,
    isFullScreen: Bool,
    send: @escaping @MainActor (ViewerEvent) -> Void
  ) {
    self.sidebar = sidebar
    self.isFullScreen = isFullScreen
    self.send = send
    _isShort = State(initialValue: sidebar.isShort)
  }

  /// The top bar, the sessions, scrolling when short, and the foot.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      SidebarTopBar(search: sidebar.search, isFullScreen: isFullScreen, send: send)
        .padding(.horizontal, ViewerLayout.sidebarInset)
        .padding(.bottom, ViewerLayout.topBarGap)
      ScrollView {
        SidebarSessionList(state: sidebar.sessions, send: send)
          .padding(.horizontal, ViewerLayout.sidebarInset)
          .padding(.bottom, ViewerLayout.sessionsFade)
      }
      .scrollIndicators(.automatic)
      .scrollBounceBehavior(.basedOnSize)
      .mask { fadingEdge }
      .frame(maxHeight: .infinity, alignment: .top)
      foot
        .padding(.horizontal, ViewerLayout.sidebarInset)
        .padding(.top, ViewerLayout.footGap)
    }
    .padding(.top, ViewerLayout.sidebarTopInset)
    .padding(.bottom, ViewerLayout.sidebarInset)
    .frame(maxHeight: .infinity, alignment: .top)
    .onChange(of: sidebar.isShort) { _, short in
      withAnimation(Motion.layout(Motion.state, reduceMotion: reduceMotion)) {
        isShort = short
      }
    }
  }

  /// The sessions' room, whole down to its last ``ViewerLayout/sessionsFade`` points, which fade out into the ground.
  private var fadingEdge: some View {
    VStack(spacing: .zero) {
      Rectangle()
      LinearGradient(colors: [.black, .clear], startPoint: .top, endPoint: .bottom)
        .frame(height: ViewerLayout.sessionsFade)
    }
  }

  /// The helper's health: its dot alone in a short window, else its line.
  @ViewBuilder private var foot: some View {
    if isShort {
      SidebarHelperLine(helper: sidebar.helper, isCompact: true, send: send)
        .transition(.opacity.animation(reduceMotion ? Motion.reduced : Motion.state))
    } else {
      SidebarHelperLine(helper: sidebar.helper, send: send)
        .transition(.opacity)
    }
  }
}

#if DEBUG
  #Preview("By state") {
    ViewerSidebar(sidebar: PreviewStates.viewerSidebarByState.sidebar, isFullScreen: false) { _ in }
      .frame(width: ViewerLayout.sidebarWidth, height: PreviewStates.viewerSize.height)
      .background(Color.groundRecessed)
  }

  #Preview("Short") {
    @Previewable @State var sidebar = PreviewStates.viewerSidebarByState.sidebar
    InteractivePreview {
      Toggle(String("Short"), isOn: $sidebar.isShort)
    } content: {
      ViewerSidebar(sidebar: sidebar, isFullScreen: false) { _ in }
        .frame(width: ViewerLayout.sidebarWidth, height: SampleWidth.narrow)
        .background(Color.groundRecessed)
    }
  }
#endif
