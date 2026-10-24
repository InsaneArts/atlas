import SwiftUI

/// The line under a screen's controls: what runs now, or what went wrong. It keeps its height when empty, so the
/// screen does not jump as it comes and goes.
struct WorkNoteLine: View {
  /// What the line says.
  let note: WorkNote

  /// A spinner beside what runs, the problem in orange, or nothing.
  var body: some View {
    Group {
      switch note {
      case .working(let text):
        HStack(spacing: Spacing.m) {
          Spinner()
          Text(text)
            .font(Typography.font(.callout))
            .foregroundStyle(Color.textSecondary)
        }
      case .problem(let text):
        Note(text: text, tone: .attention)
      case .none:
        Color.clear
          .frame(width: Spacing.hairline, height: Spacing.hairline)
      }
    }
    .frame(minHeight: OnboardingLayout.workNoteHeight)
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Working and problem") {
    VStack(spacing: Spacing.l) {
      WorkNoteLine(note: PreviewStates.onboardingCreateWorking.note)
      WorkNoteLine(note: .problem(String("The account could not be created.")))
    }
    .sampleSurface()
  }
#endif
