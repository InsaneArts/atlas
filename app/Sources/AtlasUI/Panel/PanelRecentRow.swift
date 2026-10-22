import SwiftUI

/// One of Recent's rows: a quiet dot, or a warning sign for what did not work, then what was done and, on the
/// right, how long ago. VoiceOver reads it as one.
struct PanelRecentRow: View {
  /// The action.
  let entry: ActivityEntry

  var body: some View {
    HStack(spacing: Spacing.m) {
      ActivityMark(kind: entry.mark)
      Text(entry.line)
        .truncationMode(.tail)
      Spacer(minLength: Spacing.m)
      Text(entry.when)
        .foregroundStyle(Color.textTertiary)
        .monospacedDigit()
    }
    .font(Typography.font(.callout))
    .lineLimit(1)
    .frame(minHeight: PanelLayout.recentRow)
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Rows") {
    VStack(spacing: .zero) {
      ForEach(PreviewStates.panelIdle.recent) { entry in
        PanelRecentRow(entry: entry)
      }
    }
    .frame(width: PanelLayout.picture.width)
    .sampleSurface()
  }
#endif
