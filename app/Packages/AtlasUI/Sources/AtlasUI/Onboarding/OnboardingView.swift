import SwiftUI

/// The setup window's content: the first reading, the permission panel beside System Settings, or one screen at a
/// time. It draws its state and sends what the person does; it decides nothing. The account form is the one value it
/// edits in place.
public struct OnboardingView<DragIcon: View>: View {
  /// What the window shows.
  private let state: OnboardingState
  /// The new helper account's form.
  private let form: Binding<OnboardingAccountForm>
  /// The permission panel's icon, which the person drags into System Settings.
  private let dragIcon: DragIcon
  /// Receives what the person does.
  private let send: @MainActor (OnboardingEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates the window's content.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - form: The new helper account's form, which the account screen edits.
  ///   - dragIcon: The permission panel's icon, which the person drags into System Settings.
  ///   - send: Receives what the person does.
  public init(
    state: OnboardingState,
    form: Binding<OnboardingAccountForm>,
    dragIcon: DragIcon,
    send: @escaping @MainActor (OnboardingEvent) -> Void
  ) {
    self.state = state
    self.form = form
    self.dragIcon = dragIcon
    self.send = send
  }

  /// What the window shows now, on the dark ground.
  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .windowSurface()
      .onChange(of: screenID) { _, _ in
        if case .screen(let screen) = state.content { announce(screen.spokenHeadline) }
      }
      .onChange(of: state.note) { _, note in
        if case .problem(let text) = note { announce(text) }
      }
  }

  /// Which screen shows, if one does: VoiceOver says its headline as it arrives.
  private var screenID: OnboardingScreen.ID? {
    guard case .screen(let screen) = state.content else { return nil }
    return screen.id
  }

  /// VoiceOver says what changed.
  ///
  /// - Parameter text: What to say.
  private func announce(_ text: String) {
    AccessibilityNotification.Announcement(text)
      .post()
  }

  /// The permission panel, the first reading, or a screen, each screen arriving as the last one leaves.
  @ViewBuilder private var content: some View {
    switch state.content {
    case .permissionPanel(let panel):
      PermissionPanel(state: panel, dragIcon: dragIcon, send: send)
        // No window animates its state, so the fade carries its own animation.
        .transition(.opacity.animation(Motion.fade))
    case .loading(let problem):
      LoadingScreen(problem: problem, send: send)
    case .screen(let screen):
      OnboardingScreenContent(
        screen: screen,
        footer: state.footer,
        note: state.note,
        isBusy: state.isBusy,
        form: form,
        send: send
      )
      .id(screen.id)
      .transition(reduceMotion ? .opacity.animation(Motion.fade) : screenTransition)
    }
  }

  /// A screen rises into place just after the last one starts to rise away.
  private var screenTransition: AnyTransition {
    .asymmetric(
      insertion: .opacity
        .combined(with: .offset(y: OnboardingLayout.entryOffset))
        .animation(
          Motion.entry
            .delay(OnboardingLayout.entryDelay)
        ),
      removal: .opacity
        .combined(with: .offset(y: OnboardingLayout.exitOffset))
        .animation(Motion.fade)
    )
  }
}

#if DEBUG
  #Preview("Welcome") {
    OnboardingView(state: PreviewStates.onboardingWelcome, form: .constant(PreviewStates.onboardingForm)) { _ in }
      .onboardingSample()
  }

  #Preview("Choose") {
    OnboardingView(state: PreviewStates.onboardingChoose, form: .constant(PreviewStates.onboardingForm)) { _ in }
      .onboardingSample()
  }

  #Preview("Create, working") {
    OnboardingView(state: PreviewStates.onboardingCreateWorking, form: .constant(PreviewStates.onboardingForm)) { _ in }
      .onboardingSample()
  }

  #Preview("Helper permission panel") {
    OnboardingView(state: PreviewStates.onboardingPermissionPanel, form: .constant(PreviewStates.onboardingForm)) {
      _ in
    }
    .onboardingSample(OnboardingLayout.side)
  }

  #Preview("Loading") {
    OnboardingView(state: PreviewStates.onboardingLoading, form: .constant(PreviewStates.onboardingForm)) { _ in }
      .onboardingSample()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.onboardingWelcome
    @Previewable @State var form = PreviewStates.onboardingForm
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.onboarding) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      OnboardingView(state: state, form: $form) { event in
        state = PreviewStates.onboarding(state, applying: event)
      }
      .frame(width: OnboardingLayout.card.width, height: OnboardingLayout.card.height)
    }
  }
#endif
