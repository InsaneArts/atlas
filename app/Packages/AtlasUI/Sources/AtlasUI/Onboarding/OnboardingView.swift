import SwiftUI

/// The setup window's content: the first reading, the permission panel beside System Settings, or one screen at a
/// time, over the footer. The footer sits outside the screens, so it never moves while they slide (v3 S1). It draws
/// its state and sends what the person does; it decides nothing. The account form is the one value it edits in
/// place.
public struct OnboardingView<DragIcon: View>: View {
  /// What the window shows.
  private let state: OnboardingState
  /// The new helper account's form.
  private let form: Binding<OnboardingAccountForm>
  /// The permission panel's icon, which the person drags into System Settings.
  private let dragIcon: DragIcon
  /// Receives what the person does.
  private let send: @MainActor (OnboardingEvent) -> Void

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

  /// What the window shows now over its footer, on the dark ground.
  public var body: some View {
    VStack(spacing: .zero) {
      ZStack {
        content
      }
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      ScreenActionButton(action: state.footer.next, send: send)
      OnboardingFooter(footer: state.footer, height: footerHeight, send: send)
    }
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

  /// The footer's height: the side panel's under the permission panel, the card's under the rest.
  private var footerHeight: CGFloat {
    if case .permissionPanel = state.content { return OnboardingLayout.sideFooterHeight }
    return OnboardingLayout.footerHeight
  }

  /// VoiceOver says what changed.
  ///
  /// - Parameter text: What to say.
  private func announce(_ text: String) {
    AccessibilityNotification.Announcement(text)
      .post()
  }

  /// The permission panel, the first reading, or a screen, each screen sliding in the way the person goes as the last
  /// one slides out (``PageStage``). The panel and the screens fade into each other, as the window changes size.
  @ViewBuilder private var content: some View {
    switch state.content {
    case .permissionPanel(let panel):
      PermissionPanel(state: panel, dragIcon: dragIcon)
        // No window animates its state, so the fade carries its own animation.
        .transition(.opacity.animation(Motion.fade))
    case .loading(let problem):
      LoadingScreen(problem: problem)
    case .screen(let screen):
      PageStage(id: screen.id, direction: state.direction) {
        OnboardingScreenContent(screen: screen, note: state.note, form: form, send: send)
      }
      .transition(.opacity.animation(Motion.fade))
    }
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

  #Preview("Main actions on the screens") {
    OnboardingView(state: PreviewStates.onboardingWelcome, form: .constant(PreviewStates.onboardingForm)) { _ in }
      .environment(\.mainActionInFooter, false)
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
