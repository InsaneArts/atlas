import SwiftUI

/// The newest actions under the picture (v3 P5, P8): "Recent" with All activity on its right as a link, then a row
/// for each. A new action pushes in from the top and the oldest folds away; under Reduce Motion they fade.
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
      .padding(.horizontal, Spacing.half)
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

  /// How a row comes and goes: a new one pushes in from the top as it fades in, the oldest fades as it folds away;
  /// under Reduce Motion both only fade.
  private var arriving: AnyTransition {
    Motion.transition(
      .asymmetric(
        insertion: AnyTransition.move(edge: .top)
          .combined(with: .opacity),
        removal: .opacity
      ),
      reduceMotion: reduceMotion
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
          PanelRecentEntry(id: "new-\(count)", line: "Clicked Save in Opie", when: "0 s", isLive: true),
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
