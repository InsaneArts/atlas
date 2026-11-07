import SwiftUI

/// An app run as one piece: its row, then while folded its problem lines and its newest pictures, or its pictures
/// opened in place; while open, its steps oldest first, packed, with 8 after the last.
struct ActivityRunBlock: View {
  /// The run.
  let run: ViewerOutlineRun
  /// Its level in the outline.
  let level: Int
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The row and what shows under it.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      ActivityRunRow(run: run, level: level, send: send)
      if run.isOpen {
        VStack(alignment: .leading, spacing: .zero) {
          ForEach(run.steps) { step in
            ActivityStepRow(step: step, level: level + 1, send: send)
              .id("step.\(step.id)")
          }
        }
        .padding(.bottom, ViewerLayout.stepsBelow)
        .transition(.opacity)
      } else {
        VStack(alignment: .leading, spacing: .zero) {
          ForEach(run.problemSteps) { step in
            ActivityStepRow(step: step, level: level + 1, send: send)
          }
          pictures
            .padding(.leading, ViewerLayout.outlineText(level: level))
        }
        .transition(.opacity)
      }
    }
    .accessibilityElement(children: .contain)
  }

  /// The run's pictures opened in place, or its newest few with their link.
  @ViewBuilder private var pictures: some View {
    if let grid = run.grid {
      ActivityPictureGrid(grid: grid, send: send)
    } else if let strip = run.strip {
      ActivityPictureStrip(strip: strip, send: send)
    }
  }
}
