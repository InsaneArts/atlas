import SwiftUI

/// The viewer window, in the onboarding's language: the helper's desktop on the dark ground, one white capsule for
/// the next step, and quiet cards of rows beside it on request. It draws its state and sends what the person does; it
/// decides nothing. The shell supplies `input`, the surface over the desktop that takes the mouse and keyboard while
/// in control, so this view never sees AppKit; previews pass none.
package struct ViewerView<Input: View>: View {
  /// What the window shows.
  private let state: ViewerState
  /// Receives what the person does.
  private let send: @MainActor (ViewerEvent) -> Void
  /// Reads the header's detail, the helper and how fresh its picture is, as the status line draws.
  private let detail: @MainActor () -> String
  /// The surface laid over the picture that takes the mouse and keyboard.
  private let input: Input
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates the window's content.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - send: Receives what the person does.
  ///   - detail: Reads the header's detail as the status line draws: the app's reads its controller, whose clock
  ///     ticks every second, so the picture's age redraws that line alone and not the window.
  ///   - input: The surface laid over the picture that takes the mouse and keyboard.
  package init(
    state: ViewerState,
    send: @escaping @MainActor (ViewerEvent) -> Void,
    detail: @escaping @MainActor () -> String,
    @ViewBuilder input: () -> Input
  ) {
    self.state = state
    self.send = send
    self.detail = detail
    self.input = input()
  }

  /// The header in the title bar, the stage over the footer, and the sidebar beside them when it fits.
  package var body: some View {
    GeometryReader { proxy in
      let room = ViewerLayout.showsSidebar(width: proxy.size.width, wanted: true)
      let sidebar = ViewerLayout.showsSidebar(width: proxy.size.width, wanted: state.header.sidebar.isOn)
      VStack(spacing: 0) {
        // The header fills the title bar, whatever its height.
        ViewerHeader(state: state.header, detail: detail, hasRoom: room, send: send)
          .frame(height: max(proxy.safeAreaInsets.top, ViewerLayout.headerHeight))
        HStack(spacing: 0) {
          VStack(spacing: 0) {
            StageView(state: ViewerLayout.stage(state.stage, besideSidebar: sidebar), send: send, input: input)
              .padding(.horizontal, Spacing.section)
              .padding(.top, Spacing.ml)
              .padding(.bottom, Spacing.group)
              .frame(maxWidth: .infinity, maxHeight: .infinity)
            ViewerFooter(state: state.footer, send: send)
          }
          .overlay {
            if let recovery = state.recovery {
              RecoveryAlert(prompt: recovery, send: send)
            }
          }
          if sidebar {
            HStack(spacing: 0) {
              SeparatorLine(axis: .vertical)
              ViewerSidebar(state: state.sidebar, send: send)
            }
            .transition(
              reduceMotion
                ? .opacity
                : .move(edge: .trailing)
                  .combined(with: .opacity)
            )
          }
        }
      }
      .ignoresSafeArea(.container, edges: .top)
      .animation(reduceMotion ? nil : Motion.slide, value: sidebar)
    }
    .windowSurface()
    .onChange(of: state.header.status.label) { _, label in announce(label) }
    .onChange(of: state.stage.notice) { _, notice in if let notice { announce(notice) } }
    .onChange(of: state.recovery?.title) { _, title in if let title { announce(title) } }
  }

  /// VoiceOver says what changed.
  ///
  /// - Parameter text: What to say.
  private func announce(_ text: String) {
    AccessibilityNotification.Announcement(text)
      .post()
  }
}

extension ViewerView where Input == EmptyView {
  /// Creates the window's content with no input surface and the state's own detail, as previews and design renders
  /// draw it.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - send: Receives what the person does.
  package init(state: ViewerState, send: @escaping @MainActor (ViewerEvent) -> Void) {
    self.init(state: state, send: send, detail: { state.header.detail }, input: { EmptyView() })
  }
}

#if DEBUG
  #Preview("Watching") {
    ViewerView(state: PreviewStates.viewerWatching) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Sidebar") {
    ViewerView(state: PreviewStates.viewerSidebar) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Sidebar and control") {
    ViewerView(state: PreviewStates.viewerSidebarControl) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Control") {
    ViewerView(state: PreviewStates.viewerControl) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Recovery") {
    ViewerView(state: PreviewStates.viewerRecovery) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Paused") {
    ViewerView(state: PreviewStates.viewerPaused) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Unavailable") {
    ViewerView(state: PreviewStates.viewerUnavailable) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Notice") {
    ViewerView(state: PreviewStates.viewerNotice) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Logged out") {
    ViewerView(state: PreviewStates.viewerLoggedOut) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Connecting") {
    ViewerView(state: PreviewStates.viewerConnecting) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Typing") {
    ViewerView(state: PreviewStates.viewerTyping) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWatching
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      ViewerView(state: state) { event in
        state = PreviewStates.viewer(state, applying: event)
      }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
    }
  }
#endif
