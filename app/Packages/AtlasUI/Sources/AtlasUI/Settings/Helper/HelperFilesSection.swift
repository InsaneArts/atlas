import SwiftUI

/// The Helper pane's "Your files" section: the home folder, private from the helper with Undo… or open with Make
/// Private, then whether agents may copy sign-ins in, and under them what this setting doesn't cover. Private and open
/// cross-fade in place, the row keeping its height and the button its width.
struct HelperFilesSection: View {
  /// What the section shows.
  let state: SettingsFilesState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// The section's name, rows and notes.
  var body: some View {
    GroupedSection(items, title: "Your files", notes: notes) { item in
      switch item {
      case .home(let home):
        ListRow(
          home.title,
          detail: home.detail,
          lead: .tile(.symbol(home.isPrivate ? "lock" : "folder")),
          trailing: .button(home.button),
          size: .compact
        ) { send(home.isPrivate ? .askOpenHome : .makeHomePrivate) }
        .disabled(!home.canRun)
        .animation(Motion.textSwap, value: home.isPrivate)
      case .signIns(let isOn):
        ListRow(
          "Copy sign-ins into the helper",
          detail: "Off keeps browser profiles, keychains, .env and .npmrc out",
          lead: .tile(.symbol("key")),
          trailing: .toggle(isOn: isOn),
          size: .compact
        ) { send(.setSignInCopies(!isOn)) }
        .disabled(!state.canChangeSignIns)
      }
    }
  }

  /// The rows Atlas can say something about.
  private var items: [HelperFilesItem] {
    [state.home.map(HelperFilesItem.home), state.signInCopies.map(HelperFilesItem.signIns)]
      .compactMap { $0 }
  }

  /// What the last change said, then what the setting doesn't cover.
  private var notes: [GroupedSectionNote] {
    [
      state.status?.sectionNote,
      .text("Only your home folder: /Users/Shared and external drives keep their own permissions."),
    ]
    .compactMap { $0 }
  }
}

#if DEBUG
  #Preview("Your files, private") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      HelperFilesSection(state: PreviewStates.settingsFilesPrivate) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Your files, open") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      HelperFilesSection(state: PreviewStates.settingsFilesOpen) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Your files, failed") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      HelperFilesSection(state: PreviewStates.settingsFilesFailed) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Your files, sign-in copies allowed") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      HelperFilesSection(state: PreviewStates.settingsFilesCopiesAllowed) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }
#endif
