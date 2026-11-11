import SwiftUI

/// An ended session's row in the sidebar, on one line: its host's small icon, centred on the axis of the live rows'
/// icons, with a spinner on its corner while its work is cleaned up; its name in the secondary colour, cut short in its
/// middle; and at the trailing edge when it started, or "Cleaning up…", its digits all one width, never cut short. A
/// new line swaps in place with no overlap (``LineSwap``). Its tooltip and VoiceOver still give its agent and the day
/// and time it started.
struct SidebarEndedLabel: View {
  /// The session.
  let row: ViewerSessionRow
  /// What the row rests on, whose colour the spinner's ring takes.
  let ground: Color

  /// The icon, the name and when it started.
  var body: some View {
    HStack(spacing: ViewerLayout.sidebarIconGap) {
      SidebarBadgedIcon(
        icon: row.icon,
        side: ViewerLayout.endedIcon,
        badge: row.badge,
        isCleaningUp: row.isCleaningUp,
        ground: ground
      )
      .padding(.leading, ViewerLayout.endedIconInset)
      Text(row.name)
        .font(Typography.font(.body))
        .foregroundStyle(Color.textSecondary)
        .truncationMode(.middle)
        .lineLimit(1)
        .frame(maxWidth: .infinity, alignment: .leading)
      if !row.line.isEmpty {
        LineSwap(row.line) { line in
          Text(line)
            .font(Typography.digits(.caption))
            .foregroundStyle(Color.textTertiary)
            .lineLimit(1)
            .fixedSize()
        }
      }
    }
    .help(Text(verbatim: row.help))
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(Text(verbatim: row.accessibilityLabel))
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .leading, spacing: Spacing.m) {
      SidebarSessionLabel(row: PreviewStates.sessionRows[2], ground: Color.groundRecessed)
      ForEach(PreviewStates.endedRows) { SidebarEndedLabel(row: $0, ground: Color.groundRecessed) }
    }
    .frame(width: ViewerLayout.rowContentWidth)
    .sampleSurface()
  }
#endif
