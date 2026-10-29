import SwiftUI

/// The sidebar toggle, in the window's own space beside the window buttons: a symbol alone in a circle, lit under the
/// pointer. It stays where it stands whether the sidebar shows or hides, so the person can press it again where it
/// was; in full screen, which hides the window buttons, it takes their place at once.
struct ViewerTitleButtons: View {
  /// How the sidebar stands as drawn now.
  let mode: ViewerSidebarMode
  /// Whether the window is in full screen.
  let isFullScreen: Bool
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void

  /// The toggle at its place.
  var body: some View {
    let origin = ViewerLayout.toggleOrigin(isFullScreen: isFullScreen)
    Button {
      send(.toggleSidebar)
    } label: {
      Image(systemName: SymbolName.sidebarLeft.rawValue)
    }
    .buttonStyle(.capsule(.glyph, size: .compact, outline: .circle))
    .help(Text(verbatim: mode == .hidden ? "Show Sidebar (⌃⌘S)" : "Hide Sidebar (⌃⌘S)"))
    .accessibilityLabel(Text(verbatim: mode == .hidden ? "Show sidebar" : "Hide sidebar"))
    .padding(.leading, origin.x)
    .padding(.top, origin.y)
  }
}

#if DEBUG
  #Preview("Beside the window buttons") {
    ViewerTitleButtons(mode: .full, isFullScreen: false) { _ in }
      .frame(
        width: ViewerLayout.sidebarWidth,
        height: ViewerLayout.sidebarTopInset * 2 + ViewerLayout.topBarHeight,
        alignment: .topLeading
      )
      .background(Color.groundRecessed)
  }
#endif
