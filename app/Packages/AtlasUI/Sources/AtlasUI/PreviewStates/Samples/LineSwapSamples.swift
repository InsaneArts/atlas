#if DEBUG
  import SwiftUI

  /// The line swap's samples: the latest action under the helper's screen, a session's icon, its words and their age,
  /// one line for each step asked for.
  struct LineSwapSamples: View {
    /// The steps whose lines are drawn, each in its own swap.
    private let steps: [Int]

    /// Creates the samples.
    ///
    /// - Parameter steps: The steps whose lines are drawn; the first three by default.
    init(steps: [Int] = [0, 1, 2]) {
      self.steps = steps
    }

    /// Each step's line, swapping as its step changes, on a card.
    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.m) {
        ForEach(steps.indices, id: \.self) { index in
          LineSwap(SampleLine.line(at: steps[index]), id: \.id) { line in
            HStack(spacing: Spacing.xsPlus) {
              SessionIcon(icon: SessionSamples.claudeIcon, side: IconSize.s)
              Text(verbatim: line.words)
                .foregroundStyle(Color.textSecondary)
              Text(verbatim: line.age)
                .foregroundStyle(Color.textTertiary)
            }
            .textStyle(.callout)
            .lineLimit(1)
          }
        }
      }
      .frame(width: SampleWidth.narrow, alignment: .leading)
      .padding(Spacing.mPlus)
      .background(Color.fillContent)
      .sampleSurface()
    }
  }
#endif
