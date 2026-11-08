import SwiftUI

/// The Helper pane's "Your files" section: the home folder, private from the helper with Undo… or open with Make
/// Private, then whether agents may copy sign-ins in, and under them what this setting doesn't cover. Private and open
/// cross-fade in place, the row keeping its height and the button its width; while a change runs, a spinner takes the
/// button's title's place, and a change that failed fades its line in under the section.
struct HelperFilesSection: View {
  /// What the section shows.
  let state: SettingsFilesState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// The section's name, rows and notes.
  var body: some View {
    GroupedSection(items, title: "Your files", notes: notes) { item in
      switch item {
      case .home(let home): homeRow(home)
      case .signIns(let isOn): signInsRow(isOn)
      }
    }
    .animation(Motion.fade, value: state.status)
  }

  /// The home folder's row: a success-toned lock while private, a folder while open, and its button.
  ///
  /// - Parameter home: The row.
  /// - Returns: The row.
  private func homeRow(_ home: SettingsFilesRow) -> some View {
    HelperFilesRow(
      symbol: home.isPrivate ? .lockFilled : .folder,
      tone: home.isPrivate ? Color.textSuccess : Color.textSecondary,
      title: home.title,
      detail: home.detail,
      swap: home.isPrivate
    ) {
      Button {
        send(home.isPrivate ? .askOpenHome : .makeHomePrivate)
      } label: {
        ZStack {
          Text(home.heldButton)
            .hidden()
          WorkingLabel(title: home.button, isWorking: home.isWorking, workingTitle: "Changing…")
            .swapsText(on: home.button)
        }
      }
      .buttonStyle(.capsule(.secondary, size: .mini, isWorking: home.isWorking))
      .fixedSize()
      .disabled(!home.canRun)
      .layoutFrame("files home button")
    }
    .layoutFrame("files home row")
  }

  /// Whether agents may copy sign-ins into the helper, with its switch.
  ///
  /// - Parameter isOn: Whether they may.
  /// - Returns: The row.
  private func signInsRow(_ isOn: Bool) -> some View {
    let title = "Copy sign-ins into the helper"
    return HelperFilesRow(
      symbol: .key,
      tone: Color.textSecondary,
      title: title,
      detail: "Off keeps browser profiles, keychains, .env and .npmrc out",
      swap: isOn
    ) {
      ListRowTrailing(trailing: .toggle(isOn: isOn), label: title) { send(.setSignInCopies(!isOn)) }
    }
    .disabled(!state.canChangeSignIns)
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
