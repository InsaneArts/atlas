import SwiftUI

/// The General page: opening at login and the menu bar's icon; how often the helper's screen updates, as segments,
/// and muting the helper; and whether Atlas notifies the person. Its values change through the binding.
struct GeneralPage: View {
  /// The page's values, shared with the Activity page.
  @Binding var state: SettingsGeneralState

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection(GeneralRow.menuBar, title: "Menu Bar") { row in
        self.row(row)
      }
      GroupedSection([GeneralRow.picture, .muteHelper], title: "Helper Screen") { row in
        self.row(row)
      }
      GroupedSection([GeneralRow.notify], title: "Notifications") { row in
        self.row(row)
      }
    }
  }

  /// One row: its words and its control.
  ///
  /// - Parameter row: The row.
  /// - Returns: The row.
  @ViewBuilder private func row(_ row: GeneralRow) -> some View {
    switch row {
    case .openAtLogin:
      ListRow(
        "Open Atlas at login",
        detail: "Atlas sits in the menu bar from the start",
        trailing: .toggle(isOn: state.opensAtLogin),
        size: .compact
      ) { state.opensAtLogin.toggle() }
      .disabled(!state.canOpenAtLogin)
    case .menuBarIcon:
      ListRowBody("Menu bar icon", detail: "Pick Screen if another app's icon looks like Atlas's", size: .compact) {
        Tabs(state: iconTabs, style: .segment) { picked in state.menuBarIcon = picked }
          .fixedSize()
      }
    case .picture:
      ListRowBody("Screen updates", detail: "How often Atlas's windows show the helper's screen", size: .compact) {
        Tabs(state: pictureTabs, style: .segment) { picked in state.picture = picked }
          .fixedSize()
      }
    case .muteHelper:
      ListRow(
        "Mute the helper",
        detail: state.soundProblem ?? "Apps in the helper stay silent on your speakers",
        trailing: .toggle(isOn: state.muteHelper),
        size: .compact
      ) { state.muteHelper.toggle() }
    case .notify:
      ListRow(
        "Notify me",
        detail: "When an action may not have finished, or the helper logs out",
        trailing: .toggle(isOn: state.notify),
        size: .compact
      ) { state.notify.toggle() }
    }
  }

  /// The menu bar's icon as segments: the Orbit, or the Orbit on a display.
  private var iconTabs: CapsuleTabsState<MenuBarIcon> {
    CapsuleTabsState(
      tabs: [CapsuleTab(id: .orbit, title: "Orbit"), CapsuleTab(id: .screen, title: "Screen")],
      selected: state.menuBarIcon,
      label: "Menu bar icon"
    )
  }

  /// The screen's choices as segments: those that can be chosen now, Paused only while a main window can pause.
  private var pictureTabs: CapsuleTabsState<SettingsPicture> {
    CapsuleTabsState(
      tabs: state.pictureOptions
        .filter(\.isEnabled)
        .map { CapsuleTab(id: $0.id, title: $0.title) },
      selected: state.picture,
      label: "Screen updates"
    )
  }
}

#if DEBUG
  #Preview("General") {
    SettingsPageSample { GeneralPage(state: .constant(PreviewStates.settingsGeneral.general)) }
  }

  #Preview("General, dark") {
    SettingsPageSample { GeneralPage(state: .constant(PreviewStates.settingsGeneral.general)) }
      .environment(\.colorScheme, .dark)
  }
#endif
