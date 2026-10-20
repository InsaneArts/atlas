import SwiftUI

/// The narrow panel that sits beside System Settings while a switch is turned on, and steps aside by itself once it
/// is.
struct GuidePanel: View {
  /// The permission and its steps.
  let state: OnboardingGuideState
  /// Receives Back.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The three steps, what the panel waits for, and Back.
  var body: some View {
    VStack(spacing: Spacing.l) {
      Eyebrow(text: state.eyebrow)
      Headline(title: "Turn on Atlas.", size: .compact)
        .padding(.bottom, Spacing.xs)
      StepRow(index: 1, title: "Find Atlas in the list", detail: state.settingsPath, state: .done)
      StepRow(
        index: 2,
        title: "Turn its switch on",
        detail: "Confirm with Touch ID or your password",
        state: state.turnOn
      )
      StepRow(index: 3, title: "Come back here", detail: "Atlas notices on its own", state: state.comeBack)
      Spacer(minLength: Spacing.m)
      ProgressPill(text: state.pill, done: state.isOn)
      Button("Back") { send(.closeGuide) }
        .buttonStyle(.capsule(.plain))
    }
    .padding(.horizontal, OnboardingLayout.guideSideInset)
    .padding(.top, OnboardingLayout.guideTop)
    .padding(.bottom, OnboardingLayout.guideBottom)
  }
}

#if DEBUG
  #Preview("Helper guide") {
    GuidePanel(state: PreviewStates.onboardingGuideState) { _ in }
      .onboardingSample(OnboardingLayout.side)
  }
#endif
