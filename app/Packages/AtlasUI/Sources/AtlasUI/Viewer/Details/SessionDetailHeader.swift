import SwiftUI

/// The chosen host session at the top of Details: its host's icon, its name, and under it its state and line with
/// its mark, in its tone's colour. VoiceOver reads it as one.
struct SessionDetailHeader: View {
  /// The session.
  let detail: ViewerSessionDetail

  /// The icon beside the name over the state.
  var body: some View {
    HStack(spacing: Spacing.m) {
      SessionIcon(icon: detail.icon, side: IconSize.l)
      VStack(alignment: .leading, spacing: Spacing.half) {
        Text(detail.name)
          .font(Typography.font(.body, weight: .semibold))
          .foregroundStyle(Color.textPrimary)
          .truncationMode(.middle)
        HStack(spacing: Spacing.xsPlus) {
          StatusDot(mark: .dot, tone: detail.tone, size: .small)
          Text(detail.state)
            .font(Typography.font(.caption, weight: .medium))
            .foregroundStyle(detail.tone.stateColor)
            .truncationMode(.tail)
        }
      }
      .lineLimit(1)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Samples") {
    if let detail = PreviewStates.viewerSessionDetail.details?.hostSession {
      SessionDetailHeader(detail: detail)
        .frame(width: ViewerLayout.detailsWidth)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var ended = false
    if let detail = PreviewStates.viewerSessionDetail.details?.hostSession {
      InteractivePreview {
        Toggle(String("Ended"), isOn: $ended)
      } content: {
        SessionDetailHeader(
          detail: {
            var shown = detail
            if ended {
              shown.state = String("Ended 20 min ago")
              shown.tone = .quiet
            }
            return shown
          }()
        )
        .frame(width: ViewerLayout.detailsWidth)
      }
    }
  }
#endif
