import SwiftUI

/// One of Sessions' rows, a button that opens the session's activity: the host's icon, the session's name over what it
/// does or did last, orange when it asks something of the person, and its state at the trailing edge, a dot or a ring
/// in its tone's colour. A new line or state swaps in place with no overlap (``LineSwap``), the same under Reduce
/// Motion. VoiceOver reads
/// it as one, in the words the presenter gives it.
struct PanelSessionRow: View {
  /// The session.
  let entry: PanelSessionEntry
  /// Receives the press.
  let send: @MainActor (PanelEvent) -> Void

  /// The icon, the name over the line, and the state.
  var body: some View {
    Button {
      send(.openSession(entry.id))
    } label: {
      HStack(spacing: PanelLayout.sessionGap) {
        SessionIcon(icon: entry.icon, side: PanelLayout.sessionIcon)
        words
        state
      }
      .padding(PanelLayout.sessionInset)
      .frame(minHeight: PanelLayout.sessionRow)
    }
    .buttonStyle(.row(corner: PanelLayout.sessionCorner))
    .help("Show its activity in Atlas")
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(entry.label)
    .accessibilityHint("Shows its activity in Atlas")
  }

  /// The name over the line, each on one line cut short at its end.
  private var words: some View {
    VStack(alignment: .leading, spacing: .zero) {
      Text(entry.name)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
      if !entry.line.isEmpty {
        LineSwap(entry, id: \.line) { entry in
          Text(entry.line)
            .font(Typography.font(.callout))
            .foregroundStyle(entry.isUrgent ? Color.textAttention : Color.textSecondary)
        }
      }
    }
    .lineLimit(1)
    .truncationMode(.tail)
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  /// The state's mark and word, in its tone's colour, kept whole.
  private var state: some View {
    LineSwap(entry, id: \.state) { entry in
      HStack(spacing: Spacing.xsPlus) {
        StatusDot(mark: entry.mark, tone: entry.tone, size: .small)
        Text(entry.state)
          .font(Typography.font(.callout, weight: .medium))
          .foregroundStyle(entry.tone.stateColor)
          .lineLimit(1)
      }
    }
    .fixedSize()
  }
}

#if DEBUG
  #Preview("Rows") {
    VStack(spacing: Spacing.half) {
      ForEach(PreviewStates.panelSessions.sessions) { entry in
        PanelSessionRow(entry: entry) { _ in }
      }
    }
    .frame(width: PanelLayout.picture.width)
    .sampleSurface()
  }

  #Preview("Needs you") {
    VStack(spacing: Spacing.half) {
      ForEach(PreviewStates.panelSessionsNeedsYou.sessions) { entry in
        PanelSessionRow(entry: entry) { _ in }
      }
    }
    .frame(width: PanelLayout.picture.width)
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var index = 0
    InteractivePreview {
      Button(String("Next state")) { index += 1 }
    } content: {
      let states = SessionSamples.claudeStates
      PanelSessionRow(entry: states[index % states.count]) { _ in }
        .frame(width: PanelLayout.picture.width)
    }
  }
#endif
