import SwiftUI

/// Log in to the helper once, to set Atlas up there.
struct SwitchScreen: View {
  /// The steps and what happens next.
  let state: OnboardingSwitchScreenState
  /// The line under the steps while a command runs or failed.
  let note: WorkNote
  /// Whether a command runs.
  let isBusy: Bool
  /// Receives Lock and Switch.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The three steps, Lock and Switch until the helper is on screen, then what happens next.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.listColumn) {
      Eyebrow(text: "Helper account")
      Headline(title: state.title, tail: OnboardingScreen.switchTail)
      VStack(spacing: Spacing.m) {
        StepRow(index: 1, title: "Lock and switch", detail: state.logInDetail, state: state.progress.logIn)
        StepRow(
          index: 2,
          title: state.openAtlasTitle,
          detail: "In Applications, or with Spotlight",
          state: state.progress.openAtlas
        )
        StepRow(
          index: 3,
          title: "Allow Atlas twice, then switch back",
          detail: "Atlas there shows you how",
          state: state.progress.allow
        )
      }
      .padding(.top, Spacing.xs)
      if state.showsLockButton {
        Button("Lock and Switch") { send(.lockAndSwitch) }
          .buttonStyle(.capsule(size: .large))
          .keyboardShortcut(.defaultAction)
          .disabled(isBusy)
      }
      if let waiting = state.waiting {
        ProgressPill(text: waiting)
      } else {
        WorkNoteLine(note: note)
      }
    }
  }
}

#if DEBUG
  #Preview("Switch") {
    SwitchScreen(state: PreviewStates.onboardingSwitchScreen, note: .none, isBusy: false) { _ in }
      .onboardingSample()
  }
#endif
