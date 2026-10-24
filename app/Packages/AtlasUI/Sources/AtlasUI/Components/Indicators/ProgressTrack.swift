import SwiftUI

/// How far through the steps the person is, as a short track (v3 D8, S10): finished steps light grey, the current
/// step longer and in the accent, the steps to come faint. Moving on stretches the current segment as one piece and
/// crossfades the colours.
struct ProgressTrack: View {
  /// How a segment of the track stands.
  enum Segment: Sendable {
    /// A step already finished.
    case done
    /// The step to do now.
    case current
    /// A step still to come.
    case upcoming
  }

  /// The current step, from 0.
  let index: Int
  /// How many steps there are.
  let count: Int
  /// What VoiceOver says for the track, such as "Step 2 of 6", from the owner's words.
  let label: String
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// One segment per step, in a row.
  var body: some View {
    HStack(spacing: Spacing.xs) {
      // A segment is a place in the row, not an item: its position is all it is, so the position is its identity.
      ForEach(0..<count, id: \.self) { step in
        let segment = Self.segment(step, current: index)
        Capsule()
          .fill(Self.fill(segment))
          .frame(width: Self.width(segment), height: Layout.height)
      }
    }
    .animation(Motion.animation(Motion.indicator, reduceMotion: reduceMotion, reduced: Motion.reduced), value: index)
    .accessibilityElement()
    .accessibilityLabel(label)
  }

  /// How a step's segment stands against the current step.
  ///
  /// - Parameters:
  ///   - step: The segment's step, from 0.
  ///   - current: The current step, from 0.
  /// - Returns: Done before the current step, current at it, upcoming after it.
  nonisolated static func segment(_ step: Int, current: Int) -> Segment {
    if step < current { return .done }
    return step == current ? .current : .upcoming
  }

  /// A segment's width: the current one is the long one.
  ///
  /// - Parameter segment: How the segment stands.
  /// - Returns: 22 for the current step, 14 for the others.
  nonisolated static func width(_ segment: Segment) -> CGFloat {
    segment == .current ? Layout.currentWidth : Layout.width
  }

  /// A segment's fill.
  ///
  /// - Parameter segment: How the segment stands.
  /// - Returns: Light grey once done, the accent for the current step, faint for those to come.
  nonisolated static func fill(_ segment: Segment) -> Color {
    switch segment {
    case .done: .fillTrackDone
    case .current: .accent
    case .upcoming: .fillTrack
    }
  }
}

/// v3's track (`.track i`).
private enum Layout {
  /// 14: a segment's width.
  static let width: CGFloat = 14
  /// 22: the current segment's width.
  static let currentWidth: CGFloat = 22
  /// 4: a segment's height.
  static let height: CGFloat = 4
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.progressTrack)
  }

  #Preview("Interactive") {
    @Previewable @State var index = 0
    InteractivePreview {
      Stepper(String("Step \(index + 1)"), value: $index, in: 0...5)
    } content: {
      ProgressTrack(index: index, count: 6, label: String("Step \(index + 1) of 6"))
    }
  }
#endif
