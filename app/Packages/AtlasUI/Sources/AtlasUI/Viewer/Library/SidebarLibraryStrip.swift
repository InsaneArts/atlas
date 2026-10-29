import SwiftUI

/// The Library folded into one row at the sidebar's foot, as a short window leaves it: Activity, Screenshots and Apps
/// as circles round their symbols, the chosen one held, the unseen dot in Activity's corner, and the helper's dot at
/// the row's end, which opens the helper's commands. Each circle's tooltip names its list and shortcut, and choosing
/// the chosen one again shows the Screen.
struct SidebarLibraryStrip: View {
  /// The Library's rows, in order.
  let rows: [ViewerLibraryRow]
  /// The helper's health.
  let helper: ViewerHelperLine
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The circles, then the helper's dot.
  var body: some View {
    HStack(spacing: ViewerLayout.libraryCircleGap) {
      ForEach(rows) { row in
        circle(row)
      }
      Spacer(minLength: .zero)
      SidebarHelperLine(helper: helper, isCompact: true, send: send)
    }
    .padding(.horizontal, ViewerLayout.sidebarRowInset)
    .padding(.vertical, ViewerLayout.libraryRowInset)
    .frame(width: ViewerLayout.pillWidth)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Library"))
  }

  /// A list's circle: its symbol, held while its list shows, with the unseen dot in its corner.
  ///
  /// - Parameter row: The list's row.
  /// - Returns: The circle.
  private func circle(_ row: ViewerLibraryRow) -> some View {
    Button {
      send(.chooseLibrary(row.place))
    } label: {
      Image(systemName: row.place.symbol.rawValue)
    }
    .buttonStyle(.capsule(.glyph, size: .compact, outline: .circle, isOn: row.isChosen))
    .overlay(alignment: .topTrailing) {
      ViewerUnseenDot(unseen: row.unseen)
        .padding(ViewerLayout.libraryCircleDot)
        .allowsHitTesting(false)
    }
    .help(row.help)
    .accessibilityLabel(Text(row.title))
    .accessibilityAddTraits(row.isChosen ? .isSelected : [])
  }
}

#if DEBUG
  #Preview("Activity chosen") {
    SidebarLibraryStrip(
      rows: PreviewStates.viewerActivity.sidebar.library,
      helper: PreviewStates.viewerActivity.sidebar.helper
    ) { _ in }
    .padding(.horizontal, ViewerLayout.sidebarInset)
    .frame(width: ViewerLayout.sidebarWidth)
    .background(Color.groundRecessed)
  }
#endif
