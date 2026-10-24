import SwiftUI

/// Setup is done: where Atlas lives now, and whether it opens at login.
struct DoneScreen: View {
  /// What the screen shows.
  let state: OnboardingDoneScreenState
  /// The line under the setting.
  let note: WorkNote
  /// Receives the Open at Login switch.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The icon, what happens from now on, the setting and the work note.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.narrowColumn) {
      AppIcon(size: OnboardingLayout.doneIcon)
      Headline(title: OnboardingScreen.doneTitle)
        .padding(.top, Spacing.s)
      Lede(text: state.lede)
      CardGroup {
        SettingRow(
          title: "Open Atlas at login",
          detail: "So Atlas is there whenever you are.",
          isOn: Binding(get: { state.opensAtLogin }, set: { send(.setOpenAtLogin($0)) })
        )
      }
      .padding(.top, Spacing.xsPlus)
    } bottom: {
      WorkNoteLine(note: note)
      Note(text: "Getting Started… in the menu bar brings this window back.")
    }
  }
}

#if DEBUG
  #Preview("Done") {
    DoneScreen(state: PreviewStates.onboardingDoneScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif
