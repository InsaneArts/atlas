import SwiftUI

/// Numbered steps in one group (v3 D6): each a ``StepRow``, joined by a rail that turns green below each finished step
/// (v3 S3, X8).
struct StepList: View {
  /// The steps, in order.
  let steps: [Step]

  /// The steps in their group.
  var body: some View {
    GroupedList(Self.placed(steps)) { placed in
      StepRow(
        index: placed.id,
        title: placed.step.title,
        detail: placed.step.detail,
        state: placed.step.state,
        above: placed.above,
        continues: placed.continues
      )
    }
  }

  /// Each step numbered from 1, with the state of the step above it and whether one follows.
  ///
  /// - Parameter steps: The steps, in order.
  /// - Returns: The steps, placed.
  nonisolated static func placed(_ steps: [Step]) -> [Placed] {
    steps.indices.map { index in
      Placed(
        id: index + 1,
        step: steps[index],
        above: index > steps.startIndex ? steps[index - 1].state : nil,
        continues: index < steps.endIndex - 1
      )
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.step)
  }

  #Preview("Interactive") {
    @Previewable @State var finished = 0
    InteractivePreview {
      Stepper(String("Finished: \(finished)"), value: $finished, in: 0...3)
    } content: {
      StepList(
        steps: (1...3).map { number in
          StepList.Step(
            title: String("Step \(number)"),
            detail: String("Where or how"),
            state: number <= finished ? .done : number == finished + 1 ? .current : .waiting
          )
        }
      )
      .frame(width: SampleWidth.column)
    }
  }
#endif
