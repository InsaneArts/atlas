import SwiftUI

/// One setup screen, filling the window above the footer. The footer is not part of it, so it stays put while the
/// screens change (v3 S1).
struct OnboardingScreenContent: View {
  /// The screen and what it shows.
  let screen: OnboardingScreen
  /// The line under its controls.
  let note: WorkNote
  /// The new helper account's form.
  @Binding var form: OnboardingAccountForm
  /// Receives what the person does.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The screen, below the title bar.
  var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .padding(.top, OnboardingLayout.contentTop)
  }

  /// The screen this is.
  @ViewBuilder private var content: some View {
    switch screen {
    case .welcome:
      WelcomeScreen()
    case .choose(let state):
      ChooseScreen(state: state, note: note, send: send)
    case .create(let state):
      CreateScreen(state: state, form: $form)
    case .switchToHelper(let state):
      SwitchScreen(state: state, note: note)
    case .finishing(let state):
      FinishingScreen(state: state)
    case .agents(let state):
      AgentsScreen(state: state, note: note, send: send)
    case .done(let state):
      DoneScreen(state: state, note: note, send: send)
    case .allow(let state):
      AllowScreen(state: state, note: note)
    case .permissions(let state):
      PermissionsScreen(state: state, send: send)
    case .helperDone(let state):
      HelperDoneScreen(state: state, note: note)
    case .blocked(let reason):
      BlockedScreen(reason: reason)
    }
  }
}

#if DEBUG
  #Preview("Create") {
    @Previewable @State var form = PreviewStates.onboardingForm
    OnboardingScreenContent(
      screen: .create(PreviewStates.onboardingCreateScreen),
      note: .none,
      form: $form
    ) { _ in }
    .onboardingSample()
  }
#endif
