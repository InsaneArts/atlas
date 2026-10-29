import SwiftUI

/// A host session's row in the sessions column: its host's icon beside its name over its state and line, the line in
/// its tone's colour, blue while it acts and orange while it needs the person. An ended session's row is dimmed. A new
/// line swaps in place by opacity, the same under Reduce Motion.
struct SidebarSessionLabel: View {
  /// The session.
  let row: ViewerSessionRow
  /// Whether the session ended, which dims its row.
  var isEnded = false

  /// The icon beside the name over the state.
  var body: some View {
    HStack(spacing: ViewerLayout.sidebarIconGap) {
      SessionIcon(icon: row.icon, side: ViewerLayout.sidebarIcon)
        .opacity(isEnded ? Opacity.muted : 1)
      VStack(alignment: .leading, spacing: ViewerLayout.sidebarLineGap) {
        Text(row.name)
          .font(Typography.font(.callout, weight: .semibold))
          .foregroundStyle(isEnded ? Color.textSecondary : Color.textPrimary)
          .truncationMode(.middle)
        Text(row.state)
          .font(Typography.font(.caption))
          .foregroundStyle(isEnded ? Color.textTertiary : row.tone.stateColor)
          .truncationMode(.tail)
          .swapsText(on: row.state)
      }
      .lineLimit(1)
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .help(row.state)
  }
}

#if DEBUG
  #Preview("Samples") {
    if let sessions = PreviewStates.viewerSessionsEnded.sessions {
      VStack(alignment: .leading, spacing: Spacing.m) {
        ForEach(sessions.live) { SidebarSessionLabel(row: $0) }
        ForEach(sessions.ended) { SidebarSessionLabel(row: $0, isEnded: true) }
      }
      .frame(width: ViewerLayout.sidebarWidth)
      .sampleSurface()
    }
  }
#endif
