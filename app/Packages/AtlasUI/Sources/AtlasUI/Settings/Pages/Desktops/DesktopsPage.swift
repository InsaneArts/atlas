import SwiftUI

/// The Desktops page, in Helper's place once more than one desktop is set up: each desktop, opening its own page,
/// then Add a Desktop… under the list, as Accounts sets its add buttons, and what the last change said.
struct DesktopsPage: View {
  /// What the page shows.
  let state: SettingsDesktopsState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void

  /// What it draws, from its state.
  var body: some View {
    VStack(alignment: .trailing, spacing: Spacing.sPlus) {
      // The window's title already says Desktops, so the list needs no heading of its own.
      GroupedSection(state.rows) { row in
        DesktopListRow(row: row) { send(.openDesktop(row.id)) }
      }
      AddDesktopButton(send: send)
      GroupedSectionNotes(notes: [state.notice?.sectionNote].compactMap { $0 })
        .frame(maxWidth: .infinity, alignment: .leading)
    }
  }
}

#if DEBUG
  #Preview("Desktops") {
    SettingsPageSample {
      if let desktops = PreviewStates.settingsDesktops.desktops { DesktopsPage(state: desktops) { _ in } }
    }
  }
#endif
