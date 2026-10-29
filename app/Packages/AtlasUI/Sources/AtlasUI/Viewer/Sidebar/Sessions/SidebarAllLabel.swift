import SwiftUI

/// All sessions' row: the first live sessions' icons stacked, the words, and how many sessions are live at the
/// trailing edge, while any is.
struct SidebarAllLabel: View {
  /// The icons it stacks, at most three.
  let icons: [RowIcon]
  /// How many sessions are live.
  let count: Int

  /// The stacked icons, the words and the count.
  var body: some View {
    HStack(spacing: ViewerLayout.sidebarIconGap) {
      SidebarIconStack(icons: icons)
      Text(verbatim: "All sessions")
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
        .lineLimit(1)
        .frame(maxWidth: .infinity, alignment: .leading)
      if count > 0 {
        Text(count, format: .number)
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textTertiary)
          .monospacedDigit()
      }
    }
    .help(Text(verbatim: "All sessions"))
  }
}

#if DEBUG
  #Preview("Samples") {
    let sessions = PreviewStates.viewerSessions.sidebar.sessions
    SidebarAllLabel(icons: sessions.icons, count: sessions.liveCount)
      .frame(width: ViewerLayout.rowContentWidth)
      .sampleSurface()
  }
#endif
