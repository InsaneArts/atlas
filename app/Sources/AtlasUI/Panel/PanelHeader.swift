import SwiftUI

/// The panel's title, with the helper's mode beside it and the reason for it below.
struct PanelHeader: View {
  /// The helper's mode.
  let status: StatusSummary
  /// Why the helper is in its mode; nil hides the line.
  let reason: String?

  /// The mark, the title and the badge on one line, then the reason.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.xs) {
      HStack(spacing: Spacing.ml) {
        OrbitMark(size: PanelLayout.mark)
        Text("Atlas")
          .font(Typography.font(.headline, weight: .semibold))
        Spacer()
        StatusBadge(label: status.label, tone: status.tone)
      }
      if let reason {
        Text(reason)
          .font(Typography.font(.note))
          .foregroundStyle(Color.textSecondary)
          .lineLimit(2)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
  }
}

#if DEBUG
  #Preview("Watching") {
    PanelHeader(status: PreviewStates.panelWatching.status, reason: PreviewStates.panelWatching.reason)
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
