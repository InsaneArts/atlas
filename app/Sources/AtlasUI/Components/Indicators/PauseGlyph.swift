import SwiftUI

/// The picture's pause button's glyph: pause while pictures arrive, play once they are paused, as the button offers
/// the other.
struct PauseGlyph: View {
  /// Whether the picture is paused.
  let isPaused: Bool

  var body: some View {
    Image(systemName: isPaused ? "play.fill" : "pause.fill")
      .accessibilityHidden(true)
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.l) {
      PauseGlyph(isPaused: false)
      PauseGlyph(isPaused: true)
    }
    .sampleSurface()
  }
#endif
