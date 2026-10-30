import SwiftUI

/// The picture's pause in its slot in the footer: a symbol alone, lit under the pointer, held while the picture is
/// paused. Pause turns into play at once, each drawn centred in the circle's square, with no replace effect, which
/// leaves the new symbol in the old one's box, off the circle's centre, and no crossfade, which would draw the two
/// together; only the held fill eases. It pauses the picture only; work in the helper goes on.
struct ViewerPauseSlot: View {
  /// The pause; nil while there is no picture to pause.
  let pause: ViewerToggle?
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The pause or play button, in its slot.
  var body: some View {
    Button {
      send(.togglePause)
    } label: {
      Image(systemName: (isPaused ? SymbolName.play : SymbolName.pause).rawValue)
        .contentTransition(.identity)
    }
    .buttonStyle(.capsule(.glyph, size: .small, outline: .circle, isOn: isPaused))
    .animation(ViewerMainButton.fillMotion(reduceMotion: reduceMotion), value: isPaused)
    .help(pause?.help ?? "")
    .accessibilityLabel(pause?.label ?? "")
    .actionSlot(pause != nil)
  }

  /// Whether the picture is paused, which draws play on the held fill.
  private var isPaused: Bool {
    pause?.isOn == true
  }
}

#if DEBUG
  #Preview("Live and paused") {
    HStack(spacing: Spacing.s) {
      ViewerPauseSlot(pause: PreviewStates.viewerWorking.footer.controls.pause) { _ in }
      ViewerPauseSlot(pause: PreviewStates.viewerPaused.footer.controls.pause) { _ in }
    }
    .padding()
    .sampleSurface()
  }
#endif
