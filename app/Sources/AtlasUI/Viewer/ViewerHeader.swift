import SwiftUI

/// The mark and the helper's state in the middle of the title bar, the sidebar's toggle at its end.
struct ViewerHeader: View {
  /// What the title bar shows.
  let state: ViewerHeaderState
  /// Reads the helper and how fresh its picture is as the status line draws.
  let detail: @MainActor () -> String
  /// Whether the window is wide enough for the sidebar.
  let hasRoom: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The toggle at the end, clear of the window buttons, over the mark and status in the middle.
  var body: some View {
    ZStack {
      HStack {
        Spacer()
        Button {
          send(.toggleSidebar)
        } label: {
          Image(systemName: "sidebar.right")
        }
        .buttonStyle(.circle(.small, isOn: state.sidebar.isOn && hasRoom))
        .keyboardShortcut("i", modifiers: [.command, .option])
        .disabled(!hasRoom)
        .help(hasRoom ? state.sidebar.help : "Make the window wider to show the sidebar")
        .accessibilityLabel(state.sidebar.label)
      }
      // Clear of the window buttons, which full screen hides.
      .padding(.leading, state.isFullScreen ? ViewerLayout.barInset : ViewerLayout.windowButtonsInset)
      .padding(.trailing, Spacing.xxl)
      HStack(spacing: Spacing.ml) {
        OrbitMark(size: ViewerLayout.mark)
        StatusLine(state: state, detail: detail)
      }
    }
  }
}

#if DEBUG
  #Preview("Watching") {
    let header = PreviewStates.viewerWatching.header
    ViewerHeader(state: header, detail: { header.detail }, hasRoom: true) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: ViewerLayout.headerHeight)
      .sampleSurface()
  }
#endif
