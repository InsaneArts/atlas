import SwiftUI

/// The sidebar's top bar: the window buttons' place at its leading edge, which AppKit draws and which never moves, and
/// the sidebar toggle's beside them (``ViewerTitleButtons`` draws it, in the window's own space), then the search at
/// its trailing edge.
struct SidebarTopBar: View {
  /// The search.
  let search: ViewerSearchState
  /// Whether the window is in full screen, which hides the window buttons.
  let isFullScreen: Bool
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The bar, the search at its trailing edge.
  var body: some View {
    SidebarSearchField(search: search, send: send)
      .padding(.leading, ViewerLayout.searchLeading(isFullScreen: isFullScreen))
      .frame(width: ViewerLayout.pillWidth, height: ViewerLayout.topBarHeight)
  }
}
