import SwiftUI

/// Setup is done: the icon settles into place once, where Atlas lives now, and two choices in one group: whether it
/// opens at login, and whether the home folder stays private from the helper. Done is the footer's.
struct DoneScreen: View {
  /// What the screen shows.
  let state: OnboardingDoneScreenState
  /// The status under the setting.
  let note: WorkNote
  /// Receives the two switches.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The icon, what happens from now on, the settings, the status and how to come back.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(title: OnboardingScreen.doneTitle, lede: state.lede) {
        SuccessIcon(tile: OnboardingLayout.doneTile)
      }
      GroupedList(Self.settings(state)) { setting in
        ListRow(setting.title, detail: setting.detail, trailing: setting.trailing) {
          send(Self.toggled(setting.id, in: state))
        }
        .disabled(!state.isEnabled)
      }
      .controlSize(.small)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      StatusSlot(note: note)
      Note(text: "Getting Started… in Atlas’s menu brings this window back.")
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }

  /// The two settings, each with its switch: Open Atlas at login, then keeping the home folder private.
  ///
  /// - Parameter state: The done screen.
  /// - Returns: The settings' rows.
  static func settings(_ state: OnboardingDoneScreenState) -> [ListRowItem<Int>] {
    [
      ListRowItem(
        id: 0,
        title: "Open Atlas at login",
        detail: "So Atlas is there whenever you are.",
        trailing: .toggle(isOn: state.opensAtLogin)
      ),
      ListRowItem(
        id: 1,
        title: "Keep my home folder private from the helper",
        detail: "Agents still open your builds: Atlas copies them in first.",
        trailing: .toggle(isOn: state.keepsHomePrivate)
      ),
    ]
  }

  /// What pressing a setting's row sends: its switch, turned the other way.
  ///
  /// - Parameters:
  ///   - id: The setting's row.
  ///   - state: The done screen.
  /// - Returns: The event.
  static func toggled(_ id: Int, in state: OnboardingDoneScreenState) -> OnboardingEvent {
    id == 0 ? .setOpenAtLogin(!state.opensAtLogin) : .setKeepHomePrivate(!state.keepsHomePrivate)
  }
}

#if DEBUG
  #Preview("Done") {
    DoneScreen(state: PreviewStates.onboardingDoneScreen, note: .none) { _ in }
      .onboardingSample()
  }
#endif
