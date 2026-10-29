import SwiftUI

/// The Library at the sidebar's foot, under its heading: Activity, Screenshots and Apps, each a row that shows its list
/// in the card, the chosen one on its raised pill, which moves to another row at once as the choice changes; choosing
/// the chosen row again shows the Screen. It stays at the foot while the sessions scroll.
struct SidebarLibrary: View {
  /// The rows, in order.
  let rows: [ViewerLibraryRow]
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The heading and the rows.
  var body: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.sidebarRowGap) {
      heading
      ForEach(rows) { row in
        SidebarRow(isSelected: row.isChosen, height: ViewerLayout.libraryRowHeight) {
          send(.chooseLibrary(row.place))
        } content: {
          SidebarLibraryLabel(row: row)
        }
        .help(row.help)
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Library"))
  }

  /// The Library's heading, in line with the symbols.
  private var heading: some View {
    GroupLabel(text: "Library")
      .lineLimit(1)
      .fixedSize()
      .padding(.horizontal, ViewerLayout.headingInset.leading)
      .padding(.bottom, ViewerLayout.headingInset.bottom)
      .frame(width: ViewerLayout.pillWidth, alignment: .leading)
  }
}

#if DEBUG
  #Preview("Activity chosen") {
    SidebarLibrary(rows: PreviewStates.viewerActivity.sidebar.library) { _ in }
      .padding(.horizontal, ViewerLayout.sidebarInset)
      .frame(width: ViewerLayout.sidebarWidth)
      .background(Color.groundRecessed)
  }
#endif
