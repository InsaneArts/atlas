import SwiftUI

/// Setup is done: where Atlas lives now, and whether it opens at login. Done is the footer's.
struct DoneScreen: View {
  /// What the screen shows.
  let state: OnboardingDoneScreenState
  /// The line under the setting.
  let note: WorkNote
  /// Receives the Open at Login switch.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The icon, what happens from now on, the setting, the work note and how to come back.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(title: OnboardingScreen.doneTitle, lede: state.lede) {
        AppIcon(size: OnboardingLayout.doneIcon)
      }
      CardGroup {
        SettingRow(
          title: "Open Atlas at login",
          detail: "So Atlas is there whenever you are.",
          isOn: Binding(get: { state.opensAtLogin }, set: { send(.setOpenAtLogin($0)) })
        )
      }
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
    } bottom: {
      WorkNoteLine(note: note)
        .padding(.top, OnboardingLayout.Rhythm.content)
        .pagePiece(.status)
      Note(text: "Getting Started… in the menu bar brings this window back.")
        .padding(.top, OnboardingLayout.Rhythm.status)
        .pagePiece(.status)
    }
  }
}

#if DEBUG
  #Preview("Done") {
    DoneScreen(state: PreviewStates.onboardingDoneScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif
