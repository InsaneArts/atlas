import SwiftUI

/// The Helper pane (v3 G4–G6): the helper as an account with its status and Switch, whether it is ready to act with
/// Check Again, and Recover and Log Out for when something goes wrong. While another action runs, the buttons wait
/// and a line with a spinner says why.
struct HelperPane: View {
  /// What the pane shows.
  let state: SettingsHelperState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
      GroupedSection([state.account]) { account in
        HelperAccountRow(account: account, buttons: state.buttons, send: send)
      }
      GroupedSection(state.ready, title: "Ready to act") { row in
        HelperReadyRow(row: row, index: index(of: row), buttons: state.buttons) { send(.checkAgain) }
      }
      GroupedSection(HelperAction.allCases, title: "When something goes wrong", notes: notes) { action in
        actionRow(action)
      }
    }
  }

  /// Recover… or Log Out…, each with what it does, waiting while it cannot run.
  ///
  /// - Parameter action: The row.
  /// - Returns: The row.
  @ViewBuilder private func actionRow(_ action: HelperAction) -> some View {
    switch action {
    case .recover:
      ListRow(
        "Recover",
        detail: "Checks the helper and starts its mouse and keyboard again",
        trailing: .button("Recover…"),
        size: .compact
      ) { send(.askRecover) }
      .disabled(!state.buttons.canRecover)
    case .logOut:
      ListRow(
        "Log out the helper",
        detail: "Its apps quit. You log in to it again to use it",
        trailing: .button("Log Out…"),
        size: .compact
      ) { send(.askLogOut) }
      .disabled(!state.buttons.canLogOut)
    }
  }

  /// The lines under the last section: why the buttons wait, then what the last command said.
  private var notes: [GroupedSectionNote] {
    [state.waiting.map { GroupedSectionNote.status(.working, $0) }, state.notice?.sectionNote]
      .compactMap { $0 }
  }

  /// A check's place in its section.
  ///
  /// - Parameter row: The check.
  /// - Returns: Its index, the first 0.
  private func index(of row: SettingsReadyRow) -> Int {
    state.ready.firstIndex { $0.id == row.id } ?? 0
  }
}

#if DEBUG
  #Preview("Helper") {
    FormColumn(insets: SettingsLayout.paneInsets) {
      HelperPane(state: PreviewStates.settingsHelper.helper) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Helper not ready") {
    FormColumn(insets: SettingsLayout.paneInsets) {
      HelperPane(state: PreviewStates.settingsHelperNotReady.helper) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }
#endif
