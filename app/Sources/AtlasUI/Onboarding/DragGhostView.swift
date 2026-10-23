import SwiftUI

/// Atlas's icon, see-through, carried by a hand from the permission panel into System Settings' list, over and over:
/// how to allow Atlas, shown rather than told. With Reduce Motion it rests where it lands. It takes no clicks, and
/// VoiceOver skips it, as the panel says the same in words.
package struct DragGhostView: View {
  /// Where it travels.
  private let state: DragGhostState
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates the ghost.
  ///
  /// - Parameter state: Where it travels.
  package init(state: DragGhostState) {
    self.state = state
  }

  /// The ghost at this moment of its loop, or landed with Reduce Motion.
  package var body: some View {
    Group {
      if reduceMotion {
        ghost(.landed)
      } else {
        TimelineView(.animation(minimumInterval: DragGhostTrack.frameInterval)) { context in
          ghost(.at(context.date.timeIntervalSinceReferenceDate))
        }
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }

  /// The icon and the hand holding it, dark with a light edge as the pointer is, so it shows over light and dark
  /// lists alike, at `track`'s place, size and opacity.
  ///
  /// - Parameter track: The moment of the loop.
  /// - Returns: The ghost.
  private func ghost(_ track: DragGhostTrack) -> some View {
    ZStack(alignment: .bottomTrailing) {
      AppIcon(size: OnboardingLayout.dragIcon)
      ZStack {
        Image(systemName: "hand.point.up.left.fill")
          .foregroundStyle(Color.scrim)
        Image(systemName: "hand.point.up.left")
          .foregroundStyle(Color.onAccent)
      }
      .font(.system(size: OnboardingLayout.ghostHand))
      .elevation(.bar)
      .offset(x: OnboardingLayout.ghostHandOffset, y: OnboardingLayout.ghostHandOffset)
    }
    .scaleEffect(track.scale)
    .opacity(track.opacity * Opacity.demonstration)
    .position(state.point(at: track.progress))
  }
}

#if DEBUG
  #Preview("Drag ghost") {
    DragGhostView(state: DragGhostState(from: CGPoint(x: 80, y: 200), to: CGPoint(x: 420, y: 140)))
      .frame(width: SampleWidth.column, height: OnboardingLayout.side.height)
      .sampleSurface()
  }
#endif
