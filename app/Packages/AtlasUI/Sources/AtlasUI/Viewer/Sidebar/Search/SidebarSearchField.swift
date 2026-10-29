import SwiftUI

/// The sidebar's search in its top bar: a circle with a magnifier until pressed, which opens into a field across the
/// bar, taking the focus; Escape empties it, and a second Escape closes it back to its circle. The field is laid out
/// at its own width from its first frame, so its prompt never reflows as it opens: the circle fades out on
/// ``Motion/swapOut`` as the field fades in on ``Motion/swapIn``, and back, the same under Reduce Motion, since only
/// opacity changes. A click anywhere outside the field gives up its focus (``SearchField``). The field takes the focus
/// as it appears, since it appears only as the person opens the search or Find opens it, beside the card or over it,
/// and a request for the focus made in the same change that brings it would come before it could hear it.
struct SidebarSearchField: View {
  /// The search.
  let search: ViewerSearchState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The circle, or the field it opens into, at the bar's trailing edge.
  var body: some View {
    ZStack(alignment: .trailing) {
      if search.isOpen {
        SearchField(
          text: search.query,
          prompt: "Search",
          focusRequests: search.focusRequests,
          focusesOnAppear: true,
          fills: true,
          onExit: { send(.closeSearch) }
        ) { query in
          send(.search(query))
        }
        .transition(Self.swap)
      } else {
        Button {
          send(.openSearch)
        } label: {
          Image(systemName: SymbolName.search.rawValue)
        }
        .buttonStyle(.capsule(.glyph, size: .compact, outline: .circle))
        .help(Text(verbatim: "Search Sessions (⌘F)"))
        .accessibilityLabel(Text(verbatim: "Search sessions"))
        .transition(Self.swap)
      }
    }
    .frame(maxWidth: .infinity, alignment: .trailing)
  }

  /// How the circle and the field come and go: fading in on ``Motion/swapIn`` and out on ``Motion/swapOut``, nothing
  /// growing or moving.
  static var swap: AnyTransition {
    .asymmetric(
      insertion: .opacity.animation(Motion.swapIn),
      removal: .opacity.animation(Motion.swapOut)
    )
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var search = ViewerSearchState()
    InteractivePreview {
      Text(verbatim: search.isOpen ? "Open: \(search.query)" : "Closed")
    } content: {
      SidebarSearchField(search: search) { event in
        switch event {
        case .openSearch:
          search.isOpen = true
          search.focusRequests += 1
        case .search(let query): search.query = query
        case .closeSearch: search = ViewerSearchState()
        default: break
        }
      }
      .frame(width: ViewerLayout.sidebarWidth / 2)
    }
  }
#endif
