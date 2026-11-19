import SwiftUI

/// How much of something long is done, such as a download, as a rounded bar: the accent over the track's grey, or the
/// done track's grey while the work is held, as after its process ended. The bar fills as its share grows.
struct MeterBar: View {
  /// How much is done, from 0 to 1; nil while it is not known, which draws the track alone.
  let fraction: Double?
  /// Whether the work is held: the bar keeps its length in grey.
  var isHeld = false
  @Environment(\.motionReduced) private var reduceMotion

  /// The track, and the bar over it from its leading edge.
  var body: some View {
    Capsule()
      .fill(Color.fillTrack)
      .overlay(alignment: .leading) {
        GeometryReader { track in
          Capsule()
            .fill(isHeld ? Color.fillTrackDone : Color.accent)
            .frame(width: track.size.width * Self.share(fraction))
        }
      }
      .clipShape(Capsule())
      .frame(height: MeterBarLayout.height)
      .animation(
        Motion.animation(Motion.indicator, reduceMotion: reduceMotion, reduced: Motion.reduced),
        value: fraction
      )
      .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: isHeld)
      .accessibilityHidden(true)
  }

  /// The share of the track the bar takes: `fraction` kept between 0 and 1, none while it is not known.
  ///
  /// - Parameter fraction: How much is done.
  /// - Returns: The share, from 0 to 1.
  nonisolated static func share(_ fraction: Double?) -> CGFloat {
    guard let fraction, fraction.isFinite else { return 0 }
    return CGFloat(min(max(fraction, 0), 1))
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: Spacing.l) {
      MeterBar(fraction: nil)
      MeterBar(fraction: 0.484)
      MeterBar(fraction: 0.484, isHeld: true)
      MeterBar(fraction: 1)
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
