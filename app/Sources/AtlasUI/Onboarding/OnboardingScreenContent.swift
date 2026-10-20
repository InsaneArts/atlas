import SwiftUI

/// One full setup screen: its content above, its footer below.
struct OnboardingScreenContent: View {
  /// The screen and what it shows.
  let screen: OnboardingScreen
  /// The bar under it.
  let footer: OnboardingFooterState
  /// The line under its controls.
  let note: WorkNote
  /// Whether a command runs, so every button that starts one waits.
  let isBusy: Bool
  /// The new helper account's form.
  @Binding var form: OnboardingAccountForm
  /// Receives what the person does.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The screen's content, filling the window above the footer.
  var body: some View {
    VStack(spacing: .zero) {
      content
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.top, OnboardingLayout.contentTop)
      OnboardingFooter(footer: footer, send: send)
    }
  }

  /// The screen this is.
  @ViewBuilder private var content: some View {
    switch screen {
    case .welcome:
      WelcomeScreen(send: send)
    case .choose(let state):
      ChooseScreen(state: state, note: note, send: send)
    case .create(let state):
      CreateScreen(state: state, form: $form)
    case .switchToHelper(let state):
      SwitchScreen(state: state, note: note, isBusy: isBusy, send: send)
    case .finishing(let state):
      FinishingScreen(state: state, send: send)
    case .agents(let state):
      AgentsScreen(state: state, note: note, isBusy: isBusy, send: send)
    case .done(let state):
      DoneScreen(state: state, note: note, send: send)
    case .allow(let state):
      AllowScreen(state: state, note: note, isBusy: isBusy, send: send)
    case .permissions(let state):
      PermissionsScreen(state: state, send: send)
    case .helperDone(let state):
      HelperDoneScreen(state: state, note: note, isBusy: isBusy, send: send)
    case .blocked(let reason):
      BlockedScreen(reason: reason, send: send)
    }
  }
}

#if DEBUG
  #Preview("Create, with its footer") {
    @Previewable @State var form = PreviewStates.onboardingForm
    OnboardingScreenContent(
      screen: .create(PreviewStates.onboardingCreateScreen),
      footer: PreviewStates.onboardingCreate.footer,
      note: .none,
      isBusy: false,
      form: $form
    ) { _ in }
    .onboardingSample()
  }
#endif
