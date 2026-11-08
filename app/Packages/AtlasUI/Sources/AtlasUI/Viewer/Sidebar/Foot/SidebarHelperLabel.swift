import SwiftUI

/// The helper row's label, laid out as a session's: Atlas's mark with its dot, the title over the line, which takes
/// the attention colour while it says what the person must do, and a small ellipsis at the trailing edge, saying a
/// click opens a menu. The ellipsis stands in the row's trailing inset, so the words keep nearly a session's room and
/// "The helper is logged out" is never cut.
struct SidebarHelperLabel: View {
  /// The helper's health.
  let helper: ViewerHelperLine

  /// The mark, the two lines and the ellipsis.
  var body: some View {
    HStack(spacing: ViewerLayout.sidebarIconGap) {
      SidebarHelperMark(tone: helper.tone)
      lines
        .padding(.trailing, ViewerLayout.helperMoreGap)
    }
    .overlay(alignment: .trailing) {
      Image(systemName: SymbolName.more.rawValue)
        .font(Typography.symbol(.row))
        .foregroundStyle(Color.textTertiary)
        .offset(x: ViewerLayout.helperMoreReach)
        .accessibilityHidden(true)
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
