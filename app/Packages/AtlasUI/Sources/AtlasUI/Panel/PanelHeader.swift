import SwiftUI

/// The panel's header: Atlas's orbit and name on the left, the helper's mark and word on the right in the word's
/// colour. VoiceOver reads the status as one.
struct PanelHeader: View {
  /// The helper's status.
  let status: StatusSummary

  var body: some View {
    HStack(spacing: Spacing.m) {
      OrbitMark(height: PanelLayout.orbit)
      Text("Atlas")
        .font(Typography.font(.body, weight: .semibold))
      Spacer(minLength: Spacing.m)
      HStack(spacing: Spacing.s) {
        StatusDot(mark: status.mark, tone: status.tone)
        Text(status.label)
          .font(Typography.font(.callout, weight: .semibold))
          .foregroundStyle(status.tone.wordColor)
      }
      .accessibilityElement(children: .combine)
      .accessibilityAddTraits(.updatesFrequently)
    }
    .lineLimit(1)
  }
}

#if DEBUG
  #Preview("Idle") {
    PanelHeader(status: PreviewStates.panelIdle.status)
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("You have control") {
    PanelHeader(status: PreviewStates.panelControl.status)
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }
#endif
