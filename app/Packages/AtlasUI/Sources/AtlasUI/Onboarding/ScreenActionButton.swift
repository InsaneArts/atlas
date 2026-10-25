import SwiftUI

/// A screen's main action drawn above the footer as a large button, as the screens drew Get Started, Lock and Switch,
/// Try Again, Check Again, Allow and Switch Back before they moved to the footer. It shows only while
/// ``SwiftUICore/EnvironmentValues/mainActionInFooter`` is off. Otherwise it draws nothing.
struct ScreenActionButton: View {
  /// The screen's main action, from the footer's state.
  let action: OnboardingFooterAction?
  /// Receives the button's event.
  let send: @MainActor (OnboardingEvent) -> Void
  @Environment(\.mainActionInFooter) private var mainActionInFooter

  /// The large button, while the footer hands it back.
  var body: some View {
    if let action, !mainActionInFooter, action.title.isScreenAction {
      Button(action.title.words) { send(action.event) }
        .buttonStyle(.capsule(size: .large))
        .disabled(!action.isEnabled)
        .keyboardShortcut(action.isDefault ? .defaultAction : nil)
        .id(action.event)
        .padding(.bottom, Spacing.l)
    }
  }
}

#if DEBUG
  #Preview("Handed back to the screen") {
    ScreenActionButton(action: PreviewStates.onboardingWelcome.footer.next) { _ in }
      .environment(\.mainActionInFooter, false)
      .sampleSurface()
  }
#endif
