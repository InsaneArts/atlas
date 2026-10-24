import SwiftUI

/// The picture's pause in its slot: pause turns into play by a symbol replace once the picture is paused, at
/// once under Reduce Motion. It pauses the picture only; work in the helper goes on.
struct ViewerPauseSlot: View {
  /// The pause; nil while there is no picture to pause.
  let pause: ViewerToggle?
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The pause or play button, in its slot.
  var body: some View {
    Button {
      send(.togglePause)
    } label: {
      Image(systemName: (pause?.isOn == true ? SymbolName.play : SymbolName.pause).rawValue)
        .contentTransition(.symbolEffect(.replace))
        .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion), value: pause?.isOn)
    }
    .buttonStyle(.capsule(.glass, size: .small, outline: .circle))
    .help(pause?.help ?? "")
    .accessibilityLabel(pause?.label ?? "")
    .actionSlot(pause != nil)
  }
}

#if DEBUG
  #Preview("Live and paused") {
    HStack(spacing: Spacing.s) {
      ViewerPauseSlot(pause: PreviewStates.viewerWorking.top.actions.pause) { _ in }
      ViewerPauseSlot(pause: PreviewStates.viewerPaused.top.actions.pause) { _ in }
    }
    .padding()
    .sampleSurface()
  }
#endif
