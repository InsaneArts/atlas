import SwiftUI

/// Log in to the helper once, to set Atlas up there. Lock and Switch is the footer's.
struct SwitchScreen: View {
  /// The steps and what happens next.
  let state: OnboardingSwitchScreenState
  /// The line under the steps while a command runs or failed.
  let note: WorkNote

  /// The three steps, then what happens next or what runs.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(label: "Helper account", title: state.title, tail: OnboardingScreen.switchTail)
      VStack(spacing: Spacing.s) {
        StepRow(index: 1, title: "Lock and switch", detail: state.logInDetail, state: state.progress.logIn)
          .lineLimit(OnboardingLayout.nameLines)
          .truncationMode(.middle)
        StepRow(
          index: 2,
          title: state.opensByItself ? "Atlas opens there by itself" : "Open Atlas there",
          detail: state.opensByItself
            ? "If macOS asks you to set anything up, choose Set Up Later"
            : "In Applications. If macOS asks you to set anything up, choose Set Up Later",
          state: state.progress.openAtlas
        )
        StepRow(
          index: 3,
          title: "Allow Atlas twice, then switch back",
          detail: "Atlas there shows you how",
          state: state.progress.allow
        )
      }
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
    } bottom: {
      status
        .padding(.top, OnboardingLayout.Rhythm.content)
        .pagePiece(.status)
    }
  }

  /// What happens next, in the pill, while nothing runs or failed; else the work note.
  @ViewBuilder private var status: some View {
    if let waiting = state.waiting {
      ProgressPill(text: waiting)
        .lineLimit(OnboardingLayout.nameLines)
        .truncationMode(.middle)
        .multilineTextAlignment(.center)
    } else {
      WorkNoteLine(note: note)
    }
  }
}

#if DEBUG
  #Preview("Switch") {
    SwitchScreen(state: PreviewStates.onboardingSwitchScreen, note: .none)
      .onboardingSample()
  }
#endif
