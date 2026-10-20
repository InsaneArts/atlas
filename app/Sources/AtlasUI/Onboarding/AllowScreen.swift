import SwiftUI

/// In the helper: whether to let the account that asked use this one through Atlas.
struct AllowScreen: View {
  /// Who asked, and what allowing means.
  let state: OnboardingAllowScreenState
  /// The line under Allow.
  let note: WorkNote
  /// Whether a command runs.
  let isBusy: Bool
  /// Receives Allow.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The asking account handing over to Atlas, what it means, and Allow.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.wideColumn) {
      HStack(spacing: Spacing.xl) {
        Disc(content: .letter(state.initial), tone: .avatar, size: .large)
        Image(systemName: "arrow.right")
          .foregroundStyle(Color.textTertiary)
        AppIcon(size: OnboardingLayout.allowIcon)
      }
      .accessibilityHidden(true)
      Eyebrow(text: state.eyebrow)
        .padding(.top, Spacing.s)
      Headline(title: state.title, tail: "through Atlas?")
      Lede(text: state.lede)
      Button("Allow") { send(.allow) }
        .buttonStyle(.capsule(size: .large))
        .keyboardShortcut(.defaultAction)
        .disabled(isBusy)
        .padding(.top, Spacing.m)
      WorkNoteLine(note: note)
      Note(text: "Atlas can be removed from both accounts at any time.")
    }
  }
}

#if DEBUG
  #Preview("Helper allow") {
    AllowScreen(state: PreviewStates.onboardingAllowScreen, note: .none, isBusy: false) { _ in }
      .onboardingSample()
  }
#endif
