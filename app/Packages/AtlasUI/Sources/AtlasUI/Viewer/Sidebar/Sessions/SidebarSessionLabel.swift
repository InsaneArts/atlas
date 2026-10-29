import SwiftUI

/// A host session's row in the sidebar: its host's icon with its badge, or a spinner while its work is cleaned up,
/// beside its name over what it last did. A new line swaps in place with no overlap (``LineSwap``), the same under
/// Reduce Motion. Its tooltip holds its name, its line and every fact about it.
struct SidebarSessionLabel: View {
  /// The session.
  let row: ViewerSessionRow
  /// What the row rests on, whose colour the badge's ring takes.
  let ground: Color

  /// The icon beside the name over the line.
  var body: some View {
    HStack(spacing: ViewerLayout.sidebarIconGap) {
      SidebarBadgedIcon(icon: row.icon, badge: row.badge, isCleaningUp: row.isCleaningUp, ground: ground)
      VStack(alignment: .leading, spacing: ViewerLayout.sidebarLineGap) {
        Text(row.name)
          .font(Typography.font(.body, weight: .semibold))
          .foregroundStyle(Color.textPrimary)
          .truncationMode(.middle)
        if !row.line.isEmpty {
          LineSwap(row.line) { line in
            Text(line)
              .font(Typography.font(.caption))
              .foregroundStyle(Color.textSecondary)
              .truncationMode(.tail)
          }
        }
      }
      .lineLimit(1)
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .help(Text(verbatim: row.help))
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .leading, spacing: Spacing.m) {
      ForEach(PreviewStates.sessionRows) { SidebarSessionLabel(row: $0, ground: Color.groundRecessed) }
      SidebarSessionLabel(row: PreviewStates.piEndedRow, ground: Color.groundRecessed)
    }
    .frame(width: ViewerLayout.rowContentWidth)
    .sampleSurface()
  }
#endif
