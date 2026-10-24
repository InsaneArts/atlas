import SwiftUI

/// Setup is done: the icon settles once with its ring (v3 S8, X11), where Atlas lives now, and whether it opens at
/// login, in one group (v3 D6). Done is the footer's.
struct DoneScreen: View {
  /// What the screen shows.
  let state: OnboardingDoneScreenState
  /// The status under the setting.
  let note: WorkNote
  /// Receives the Open at Login switch.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The icon, what happens from now on, the setting, the status and how to come back.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(title: OnboardingScreen.doneTitle, lede: state.lede) {
        SuccessIcon(tile: OnboardingLayout.doneTile)
      }
      GroupedList(Self.settings(state)) { setting in
        ListRow(setting.title, detail: setting.detail, trailing: setting.trailing) {
          send(.setOpenAtLogin(!state.opensAtLogin))
        }
      }
      .controlSize(.small)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
    } bottom: {
      if let status = StatusLine(note: note) {
        status
          .padding(.top, OnboardingLayout.Rhythm.content)
          .pagePiece(.status)
      }
      Note(text: "Getting Started… in the menu bar brings this window back.")
        .padding(.top, OnboardingLayout.Rhythm.status)
        .pagePiece(.status)
    }
  }

  /// The one setting: Open Atlas at login, with its switch.
  ///
  /// - Parameter state: The done screen.
  /// - Returns: The setting's row.
  static func settings(_ state: OnboardingDoneScreenState) -> [ListRowItem<Int>] {
    [
      ListRowItem(
        id: 0,
        title: "Open Atlas at login",
        detail: "So Atlas is there whenever you are.",
        trailing: .toggle(isOn: state.opensAtLogin)
      )
    ]
  }
}

#if DEBUG
  #Preview("Done") {
    DoneScreen(state: PreviewStates.onboardingDoneScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif
