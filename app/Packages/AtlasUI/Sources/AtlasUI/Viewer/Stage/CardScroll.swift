import SwiftUI

/// The one scroll view every list in the card scrolls in, Activity, Screenshots and Apps alike, by the rules in
/// ``CardScrollRoom``: the small screen floats over the list, which scrolls the pane's whole height under it and
/// keeps its band as room at the end it floats over, so the last row, or the first, scrolls wholly clear of it, with
/// nothing faded and nothing ended early. Headings scroll with their rows. A step to a row scrolls it into view only
/// as far as it must, clear of the small screen (``View/listSelection(_:isSelected:)``). Scrolled down, rows coming
/// in above, or the list changing around the row scrolled to, leave what the person reads where it is; at the top the
/// list stays at the top, so new rows show. The scroll bar hides while the pane fades in or out. A list supplies only
/// its content, with `scrollTargetLayout()` on the stack whose rows hold the place.
struct CardScroll<Content: View>: View {
  /// The selected row to scroll into view as the person steps to it; nil for a list with no selection.
  var reveal: CardScrollReveal?
  /// The list's content.
  @ViewBuilder let content: Content
  /// The row the person last scrolled to the top of the list.
  @State private var scrolledTo: String?
  /// Whether the list sits at its very top.
  @State private var isAtTop = true
  /// Where the selected row stands as the list scrolls.
  @State private var selection = CardScrollSelection()
  @Environment(\.miniClearance) private var clearance
  @Environment(\.paneSettled) private var paneSettled

  /// The list, scrolling under the small screen.
  var body: some View {
    ScrollViewReader { scroll in
      ScrollView {
        content
          .environment(\.listIsAtTop, isAtTop)
          .padding(CardScrollRoom.contentInsets(clearance))
          .background {
            // Measure the padded origin, so partial scrolling through the top band counts as scrolling.
            Color.clear
              .onGeometryChange(for: Bool.self) { proxy in
                proxy.frame(in: .scrollView).minY >= -Spacing.hairline
              } action: { atTop in
                isAtTop = atTop
              }
          }
          .environment(\.cardScrollSelection, selection)
      }
      .scrollIndicators(paneSettled ? .automatic : .hidden)
      .scrollPosition(id: anchor, anchor: .top)
      .onGeometryChange(for: CGFloat.self) { proxy in
        proxy.size.height
      } action: { height in
        selection.viewport = height
      }
      .onChange(of: reveal?.request) { _, _ in
        show(reveal?.id, in: scroll)
      }
    }
  }

  /// The list's scroll position (``CardScrollRoom/anchor(scrolledTo:isAtTop:)``).
  private var anchor: Binding<String?> {
    Binding(
      get: { CardScrollRoom.anchor(scrolledTo: scrolledTo, isAtTop: isAtTop) },
      set: { scrolledTo = $0 }
    )
  }

  /// Scrolls the selected row into view clear of the small screen, only as far as it must
  /// (``CardScrollRoom/revealAnchor(row:viewport:clearance:)``): at once where it stands, else once it reports where
  /// it stands, scrolling a row the list has not built yet to the edge first, which builds it.
  ///
  /// - Parameters:
  ///   - row: The selected row's identity.
  ///   - scroll: The list's scroll view.
  private func show(_ row: String?, in scroll: ScrollViewProxy) {
    guard let row else {
      selection.cancelPlacement()
      return
    }
    let stood = selection.place(row) { frame in
      guard
        let anchor = CardScrollRoom.revealAnchor(row: frame, viewport: selection.viewport, clearance: clearance)
      else { return }
      // Out of the layout pass that reported the row, as a scroll may lay the list out again.
      selection.queue(row) { scroll.scrollTo(row, anchor: anchor) }
    }
    if !stood {
      scroll.scrollTo(row, anchor: nil)
    }
  }
}
