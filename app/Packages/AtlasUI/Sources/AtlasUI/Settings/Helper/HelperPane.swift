import SwiftUI

/// The Helper pane: the helper as an account with its status and Switch, a login Atlas can close while there is
/// one, whether it is ready to act with Check Again, and Recover and Log Out for when something goes wrong. While
/// another action runs, the buttons wait and a line with a spinner names it. When Check Again ends, VoiceOver hears
/// it once.
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
          .settingsTarget(.account)
      }
      if let row = state.closeLogin {
        GroupedSection([row], notes: row.status.map { [.status(.working, $0)] } ?? []) { row in
          ListRow(row.title, detail: row.detail, trailing: .button(row.button), size: .compact) { send(row.event) }
            .disabled(!row.canRun)
            .settingsTarget(.cleanup)
        }
      }
      VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
        HelperReadyHeader { send(.checkAgain) }
        GroupedSection([HelperAction.recover], notes: readyNotes) { _ in
          SettingsDisclosureRow(
            title: "Readiness & Diagnostics",
            summary: state.readinessSummary,
            destination: .diagnostics
          ) {
            send(.navigate(.diagnostics))
          }
          .id(SettingsSearchTarget.disclosure(.diagnostics))
        }
        GroupedSection(state.ready.filter { $0.item.state == .notReady }) { row in
          HelperReadyRow(row: row, buttons: state.buttons) { send(.askRecover) }
        }
      }
      // Check Again always lasts long enough to show, so its marks need no wait before their spinners.
      .environment(\.marksSwapAtOnce, true)
      GroupedSection(
        HelperAction.allCases,
        title: SettingsSearchCatalog.recover.section,
        description: SettingsSearchCatalog.recover.sectionDetail,
        notes: notes
      ) { action in
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
      ListRowBody(
        SettingsSearchCatalog.recover.title,
        detail: SettingsSearchCatalog.recover.detail,
        size: .compact
      ) {
        SettingsAlertButton("Recover…") { send(.askRecover) }
          .buttonStyle(.capsule(.glass, size: .mini))
      }
      .disabled(!state.buttons.canRecover)
      .settingsTarget(.recover)
    case .logOut:
      ListRowBody(
        SettingsSearchCatalog.logOut.title,
        detail: SettingsSearchCatalog.logOut.detail,
        size: .compact
      ) {
        SettingsAlertButton("Log Out…") { send(.askLogOut) }
          .buttonStyle(.capsule(.glass, size: .mini))
      }
      .disabled(!state.buttons.canLogOut)
      .settingsTarget(.logOut)
    }
  }

  /// The line under "Ready to act": what Check Again found when something isn't ready.
  private var readyNotes: [GroupedSectionNote] {
    [state.readyNote?.sectionNote]
      .compactMap { $0 }
  }

  /// The lines under the last section: why the buttons wait, then what the last command said.
  private var notes: [GroupedSectionNote] {
    [state.waiting.map { GroupedSectionNote.status(.working, $0) }, state.notice?.sectionNote]
      .compactMap { $0 }
  }

}

#if DEBUG
  #Preview("Helper") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      HelperPane(state: PreviewStates.settingsHelper.helper) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Helper, a deleted account still logged in") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      HelperPane(state: PreviewStates.settingsHelperLeftover.helper) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }

  #Preview("Helper not ready") {
    FormColumn(insets: SettingsLayout.swapInsets) {
      HelperPane(state: PreviewStates.settingsHelperNotReady.helper) { _ in }
    }
    .frame(width: SettingsLayout.width)
    .sampleSurface()
  }
#endif
