import SwiftUI

/// The newest actions under the picture: "Recent" with All activity on its right as a link, then a row for each. A new
/// action fades in at the top as the rows below move down with no bounce, and the oldest fades away; under Reduce
/// Motion the rows below take a short fade instead of moving.
struct PanelRecent: View {
  /// The actions, newest first.
  let entries: [PanelRecentEntry]
  /// Receives All activity.
  let send: @MainActor (PanelEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The heading with All activity, then a row for each recent action.
  var body: some View {
    VStack(spacing: .zero) {
      HStack(alignment: .firstTextBaseline) {
        GroupLabel(text: "Recent")
        Spacer(minLength: Spacing.s)
        Button("All activity") { send(.openActivity) }
          .buttonStyle(.textLink(.small))
          .help("Open every action in Atlas")
      }
      .padding(.horizontal, PanelLayout.textInset)
      .padding(.bottom, Spacing.half)
      VStack(spacing: .zero) {
        // An entry's id is the service's id for the action, which holds as newer ones arrive.
        ForEach(entries) { entry in
          PanelRecentRow(entry: entry)
            .transition(arriving)
        }
      }
      .clipped()
      .animation(
        Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced),
        value: entries.map(\.id)
      )
    }
  }

  /// How a row comes and goes, in place: a new one fades in over ``Motion/Duration/fade`` and the oldest fades out over
  /// ``Motion/Duration/dismiss``. Only opacity changes, so Reduce Motion keeps it.
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
  #Preview("Idle") {
    PanelRecent(entries: PreviewStates.panelIdle.recent) { _ in }
      .frame(width: PanelLayout.picture.width)
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var entries = PreviewStates.panelWorking.recent
    @Previewable @State var count = 0
    InteractivePreview {
      Button(String("New action")) {
        count += 1
        entries.insert(
          PanelRecentEntry(
            id: "new-\(count)",
            icon: .app(.monogram(AppMonogram(name: "Opie"))),
            line: "Clicked Save in Opie",
            when: "0 s",
            isLive: true
          ),
          at: 0
        )
        entries = entries.prefix(3)
          .enumerated()
          .map { index, entry in
            var shown = entry
            shown.isLive = index == 0
            return shown
          }
      }
    } content: {
      PanelRecent(entries: entries) { _ in }
        .frame(width: PanelLayout.picture.width)
    }
  }
#endif
