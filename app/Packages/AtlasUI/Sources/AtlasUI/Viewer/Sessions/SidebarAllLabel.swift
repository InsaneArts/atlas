import SwiftUI

/// All sessions' row in the sessions column: the first live sessions' icons, stacked just over each other, the words,
/// and how many sessions are live at the trailing edge.
struct SidebarAllLabel: View {
  /// The icons it stacks, at most three.
  let icons: [RowIcon]
  /// How many sessions are live.
  let count: Int

  /// The stacked icons, the words and the count.
  var body: some View {
    HStack(spacing: ViewerLayout.sidebarIconGap) {
      HStack(spacing: ViewerLayout.sessionStackSpacing) {
        ForEach(Array(icons.enumerated()), id: \.offset) { _, icon in
          SessionIcon(icon: icon, side: ViewerLayout.sessionIcon)
        }
      }
      Text(verbatim: "All sessions")
        .font(Typography.font(.callout, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
        .lineLimit(1)
        .frame(maxWidth: .infinity, alignment: .leading)
      Text(count, format: .number)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textSecondary)
        .monospacedDigit()
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    if let sessions = PreviewStates.viewerSessions.sessions {
      SidebarAllLabel(icons: sessions.icons, count: sessions.live.count)
        .frame(width: ViewerLayout.sidebarWidth)
        .sampleSurface()
    }
  }
#endif
