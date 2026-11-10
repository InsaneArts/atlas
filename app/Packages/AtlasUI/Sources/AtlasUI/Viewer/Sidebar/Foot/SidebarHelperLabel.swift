import SwiftUI

/// The helper row's label, laid out as a session's: Atlas's mark with its dot and the title over the line, which takes
/// the attention colour while it says what the person must do. The words take the row's whole room, so "The helper is
/// logged out" is never cut; the row lights under the pointer and opens Settings on a click.
struct SidebarHelperLabel: View {
  /// The helper's health.
  let helper: ViewerHelperLine

  /// The mark and the two lines.
  var body: some View {
    HStack(spacing: ViewerLayout.sidebarIconGap) {
      SidebarHelperMark(tone: helper.tone)
      lines
    }
  }

  /// The title over the line.
  private var lines: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.sidebarLineGap) {
      Text(helper.title)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
      if !helper.line.isEmpty {
        Text(helper.line)
          .font(Typography.font(.caption))
          .foregroundStyle(helper.lineTone == .attention ? Color.textAttention : Color.textSecondary)
      }
    }
    .lineLimit(1)
    .truncationMode(.tail)
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

#if DEBUG
  #Preview("Every state") {
    VStack(alignment: .leading, spacing: Spacing.m) {
      ForEach(PreviewStates.viewerHelperLines, id: \.name) { sample in
        SidebarHelperLabel(helper: sample.value)
      }
    }
    .frame(width: ViewerLayout.rowContentWidth)
    .padding(Spacing.l)
    .background(Color.groundRecessed)
  }
#endif
