import SwiftUI

/// The live host sessions under the picture, in Recent's place while two or more are live: "Sessions" with All
/// activity on its right as a link, then a row for each, at most ``PanelLayout/mostSessions``, then a row that opens
/// every session when there are more. Rows keep their place by key; a new session fades in as Recent's rows do and one
/// that ends fades away, the same under Reduce Motion, while the rows below move with no bounce.
struct PanelSessions: View {
  /// The sessions, in the service's order.
  let entries: [PanelSessionEntry]
  /// The row that opens every session, such as "+2 more"; nil for none.
  let more: String?
  /// Receives a row's press and All activity.
  let send: @MainActor (PanelEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The heading with All activity, then a row for each session and the row for the rest.
  var body: some View {
    VStack(spacing: .zero) {
      HStack(alignment: .firstTextBaseline) {
        GroupLabel(text: "Sessions")
        Spacer(minLength: Spacing.s)
        Button("All activity") { send(.openActivity) }
          .buttonStyle(.textLink(.small))
          .help("Open every action in Atlas")
      }
      .padding(.horizontal, Spacing.half)
      .padding(.bottom, Spacing.half)
      VStack(spacing: Spacing.half) {
        // An entry's id is the session's key, which holds as its state changes.
        ForEach(entries) { entry in
          PanelSessionRow(entry: entry, send: send)
            .transition(arriving)
        }
        if let more {
          moreRow(more)
            .transition(arriving)
        }
      }
      .animation(
        Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced),
        value: entries.map(\.id)
      )
    }
  }

  /// The row under the sessions, its words lined up with their names, which opens every session in Atlas.
  ///
  /// - Parameter words: What it says, such as "+2 more".
  /// - Returns: The row.
  private func moreRow(_ words: String) -> some View {
    Button {
      send(.showAllSessions)
    } label: {
      Text(words)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textSecondary)
        .lineLimit(1)
        .frame(maxWidth: .infinity, minHeight: PanelLayout.recentRow, alignment: .leading)
        .padding(.leading, PanelLayout.sessionText)
    }
    .buttonStyle(.row)
    .help("Show every session in Atlas")
    .accessibilityHint("Shows every session in Atlas")
  }

  /// How a row comes and goes, in place: a new one fades in over ``Motion/Duration/fade`` and one that leaves fades
  /// out over ``Motion/Duration/dismiss``. Only opacity changes, so Reduce Motion keeps it.
  private var arriving: AnyTransition {
    .asymmetric(
      insertion: AnyTransition.opacity
        .animation(Motion.fade),
      removal: AnyTransition.opacity
        .animation(Motion.dismiss)
    )
  }
}

#if DEBUG
  #Preview("Sessions") {
    PanelSessions(entries: PreviewStates.panelSessions.sessions, more: nil) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("More than three") {
    PanelSessions(
      entries: PreviewStates.panelSessionsMore.sessions,
      more: PreviewStates.panelSessionsMore.moreSessions
    ) { _ in }
    .frame(width: PanelLayout.picture.width)
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var count = 2
    InteractivePreview {
      Stepper(String("Sessions: \(count)"), value: $count, in: 2...SessionSamples.panelEntries.count)
    } content: {
      PanelSessions(
        entries: Array(SessionSamples.panelEntries.prefix(min(count, PanelLayout.mostSessions))),
        more: count > PanelLayout.mostSessions ? "+\(count - PanelLayout.mostSessions) more" : nil
      ) { _ in }
      .frame(width: PanelLayout.picture.width)
    }
  }
#endif
