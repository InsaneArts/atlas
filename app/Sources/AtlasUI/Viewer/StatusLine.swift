import SwiftUI

/// "Agent working · Atlas · Live", with a dot in the mode's colour.
struct StatusLine: View {
  /// What the title bar shows.
  let state: ViewerHeaderState
  /// Reads the helper and how fresh its picture is, as the line draws, so a picture's age that changes by itself
  /// redraws this line alone.
  let detail: @MainActor () -> String

  /// The dot, the status, then the helper and how fresh its picture is.
  var body: some View {
    HStack(spacing: Spacing.m) {
      StatusDot(tone: state.status.tone, pulsing: state.isWorking)
      Text(state.status.label)
        .font(Typography.font(.body, weight: .semibold))
      Text(detail())
        .font(Typography.font(.body))
        .foregroundStyle(Color.textTertiary)
        .monospacedDigit()
    }
    .lineLimit(1)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.updatesFrequently)
    .accessibilityIdentifier("status")
  }
}

#if DEBUG
  #Preview("Agent working") {
    StatusLine(state: PreviewStates.viewerWatching.header) { PreviewStates.viewerWatching.header.detail }
      .sampleSurface()
  }
#endif
